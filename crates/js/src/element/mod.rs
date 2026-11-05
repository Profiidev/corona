//! Elements: `div().flex().child(..)` builds gpui elements in a Rust arena, the [`Frame`] of
//! one render. A JS [`El`] is only the index of its [`Node`] there.
//!
//! Every builder method is a host fn on the `El` prototype, made by an [`el_methods!`] table
//! that also writes its TypeScript. Typos throw a `TypeError` (`not a function`).

use std::{any::Any, cell::Cell, collections::HashMap, marker::PhantomData, mem};

use gpui_kit::{
  AnyElement, App, Context, Div, EntityId, Img, InteractiveElement, Interactivity, IntoElement,
  List, StatefulInteractiveElement, StyleRefinement, Subscription, Svg, UniformList, WeakEntity,
  Window, div,
};
use rquickjs::{
  Class, Ctx, Exception, FromJs, Function, IntoJs, JsLifetime, Persistent, Result, Value,
  class::Trace,
  function::{Rest, This},
};

use crate::{
  Opt, ScriptView, ViewHandle,
  runtime::scope::{frame, try_frame},
};

pub(crate) mod background;
pub(crate) mod components;
pub(crate) mod events;
pub(crate) mod style;

/// The most items a list, scroller, carousel or table may hold: more only exhausts memory
pub(crate) const MAX_ITEMS: u32 = 1_000_000;

/// `count` as a length, or a RangeError above [`MAX_ITEMS`]
pub(crate) fn items(ctx: &Ctx<'_>, count: u32) -> Result<usize> {
  if count > MAX_ITEMS {
    return Err(Exception::throw_range(
      ctx,
      &format!("item count {count} exceeds {MAX_ITEMS}"),
    ));
  }
  Ok(count as usize)
}

/// Observes an entity the render read, see [`track`](crate::track)
pub(crate) type Observe = Box<dyn FnOnce(&mut Context<ScriptView>) -> Subscription>;

/// A gpui element a script builds. Implement it with [`node!`](crate::node) for what the
/// element supports; methods it does not support throw.
pub trait Node: Any {
  /// Shown in errors: `"div"`, `"Button"`
  fn kind(&self) -> &'static str;
  fn style(&mut self) -> Option<&mut StyleRefinement> {
    None
  }
  fn interactivity(&mut self) -> Option<&mut Interactivity> {
    None
  }
  /// Appends a child; false if the element takes none
  fn add_child(&mut self, _child: AnyElement) -> bool {
    false
  }
  fn into_element(self: Box<Self>) -> AnyElement;
  fn into_any(self: Box<Self>) -> Box<dyn Any>;
}

/// Implements [`Node`]: `node!(Div, "div", style, interactive, parent)`, naming the gpui traits
/// (`Styled`, `InteractiveElement`, `ParentElement`) the type implements.
#[macro_export]
macro_rules! node {
  ($ty:ty, $kind:literal $(, $cap:ident)*) => {
    impl $crate::Node for $ty {
      fn kind(&self) -> &'static str {
        $kind
      }
      fn into_element(self: Box<Self>) -> $crate::gpui_kit::AnyElement {
        $crate::gpui_kit::IntoElement::into_any_element(*self)
      }
      fn into_any(self: Box<Self>) -> Box<dyn ::std::any::Any> {
        self
      }
      $($crate::node!(@$cap);)*
    }
  };
  (@style) => {
    fn style(&mut self) -> Option<&mut $crate::gpui_kit::StyleRefinement> {
      Some($crate::gpui_kit::Styled::style(self))
    }
  };
  (@interactive) => {
    fn interactivity(&mut self) -> Option<&mut $crate::gpui_kit::Interactivity> {
      Some($crate::gpui_kit::InteractiveElement::interactivity(self))
    }
  };
  (@parent) => {
    fn add_child(&mut self, child: $crate::gpui_kit::AnyElement) -> bool {
      $crate::gpui_kit::ParentElement::extend(self, [child]);
      true
    }
  };
}

node!(Div, "div", style, interactive, parent);
node!(Svg, "svg", style, interactive);
node!(Img, "image", style, interactive);
node!(UniformList, "uniform_list", style, interactive);
node!(List, "list", style);

/// What `hover(el => ..)` and its kind style: a style, no element
impl Node for StyleRefinement {
  fn kind(&self) -> &'static str {
    "style"
  }
  fn style(&mut self) -> Option<&mut StyleRefinement> {
    Some(self)
  }
  fn into_element(self: Box<Self>) -> AnyElement {
    div().into_any_element()
  }
  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }
}

/// An element whose `Interactivity` is borrowed, for gpui's builder methods on it
pub(crate) struct I(pub Interactivity);

impl InteractiveElement for I {
  fn interactivity(&mut self) -> &mut Interactivity {
    &mut self.0
  }
}
impl StatefulInteractiveElement for I {}

thread_local! {
  /// Tells the elements of one render from those of another
  static GENERATION: Cell<u32> = const { Cell::new(0) };
}

/// The elements one render builds, the handlers they hold and the entities it read
pub struct Frame {
  pub(crate) generation: u32,
  nodes: Vec<Option<Box<dyn Node>>>,
  pub(crate) handlers: Vec<Persistent<Function<'static>>>,
  pub(crate) reads: HashMap<EntityId, Observe>,
  pub(crate) view: WeakEntity<ScriptView>,
  /// Which of the view's renders this is: its own, a dialog's, a list's items
  pub(crate) slot: u64,
  #[cfg(test)]
  pub(crate) text: String,
}

impl Frame {
  pub(crate) fn new(view: WeakEntity<ScriptView>, slot: u64) -> Self {
    Self {
      generation: GENERATION.replace(GENERATION.get().wrapping_add(1)),
      nodes: Vec::new(),
      handlers: Vec::new(),
      reads: HashMap::new(),
      view,
      slot,
      #[cfg(test)]
      text: String::new(),
    }
  }

  fn push(&mut self, mut node: Box<dyn Node>) -> El {
    // gpui keeps clicks, hovers and scrolling by element id; `id(..)` names it stably
    if let Some(i) = node.interactivity().filter(|i| i.element_id.is_none()) {
      i.element_id = Some(gpui_kit::ElementId::NamedInteger(
        "auto".into(),
        self.nodes.len() as u64,
      ));
    }
    self.nodes.push(Some(node));
    El {
      id: self.nodes.len() - 1,
      generation: self.generation,
    }
  }

  /// Moves element `el` out, once.
  pub(crate) fn take(&mut self, ctx: &Ctx<'_>, el: &El) -> Result<Box<dyn Node>> {
    if el.generation != self.generation {
      return Err(Exception::throw_type(
        ctx,
        "element belongs to an earlier render; build elements in the render that uses them",
      ));
    }
    self.nodes[el.id]
      .take()
      .ok_or_else(|| Exception::throw_type(ctx, "element was already used as a child"))
  }

  fn put(&mut self, el: &El, node: Box<dyn Node>) {
    self.nodes[el.id] = Some(node);
  }

  /// The gpui element of a child
  pub(crate) fn child(&mut self, ctx: &Ctx<'_>, child: Child<'_>) -> Result<AnyElement> {
    Ok(match child {
      Child::El(el) => self.take(ctx, &el.borrow())?.into_element(),
      Child::View(view) => view.borrow().element(ctx)?,
      Child::Text(text) => {
        #[cfg(test)]
        self.text.push_str(&text);
        text.into_any_element()
      }
    })
  }

  /// Keeps `handler` for this render and returns the gpui listener that calls it with what
  /// `payload` makes of the event.
  pub fn listener<E: ?Sized + 'static, P: for<'js> IntoJs<'js> + 'static>(
    &mut self,
    handler: Function<'_>,
    payload: impl Fn(&E) -> P + 'static,
  ) -> impl Fn(&E, &mut Window, &mut App) + 'static {
    self.listener_as(false, handler, payload)
  }

  /// [`listener`](Self::listener); `gesture` handlers run with the user's activation, which
  /// `openUrl` needs.
  pub fn listener_as<E: ?Sized + 'static, P: for<'js> IntoJs<'js> + 'static>(
    &mut self,
    gesture: bool,
    handler: Function<'_>,
    payload: impl Fn(&E) -> P + 'static,
  ) -> impl Fn(&E, &mut Window, &mut App) + 'static {
    let index = self.handlers.len();
    self
      .handlers
      .push(Persistent::save(&handler.ctx().clone(), handler));
    let (view, slot) = (self.view.clone(), self.slot);
    move |event, window, cx| {
      let payload = payload(event);
      if let Some(view) = view.upgrade() {
        let _activation = Activation::set(gesture);
        view.update(cx, |view, cx| {
          view.dispatch(slot, index, payload, window, cx)
        });
      }
    }
  }
}

thread_local! {
  /// A click, mouse or key handler is running
  static ACTIVATION: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether the running JS was started by a click, mouse or key handler
pub(crate) fn activated() -> bool {
  ACTIVATION.get()
}

/// Restores the activation of the dispatch it replaced; passive handlers keep an outer one.
struct Activation(bool);

impl Activation {
  fn set(gesture: bool) -> Self {
    Activation(ACTIVATION.replace(gesture || ACTIVATION.get()))
  }
}

impl Drop for Activation {
  fn drop(&mut self) {
    ACTIVATION.set(self.0);
  }
}

/// An element under construction; its gpui element lives in the [`Frame`].
#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "Element")]
pub struct El {
  #[qjs(skip_trace)]
  id: usize,
  #[qjs(skip_trace)]
  generation: u32,
}

/// The frame of the current render, or a JS error
pub fn frame_or_throw(ctx: &Ctx<'_>) -> Result<&'static mut Frame> {
  try_frame()
    .ok_or_else(|| Exception::throw_type(ctx, "elements can only be built while rendering"))
}

/// A new element in the current render
pub fn new_el<'js>(ctx: &Ctx<'js>, node: impl Node) -> Result<Class<'js, El>> {
  new_boxed(ctx, Box::new(node))
}

/// A new element in the current render, of a boxed node
pub(crate) fn new_boxed<'js>(ctx: &Ctx<'js>, node: Box<dyn Node>) -> Result<Class<'js, El>> {
  let el = frame_or_throw(ctx)?.push(node);
  Class::instance(ctx.clone(), el)
}

/// Runs `f` on the node of `this`.
pub fn with_node<'js>(
  this: &Class<'js, El>,
  f: impl FnOnce(&Ctx<'js>, &mut Box<dyn Node>) -> Result<()>,
) -> Result<()> {
  let ctx = this.ctx();
  let el = this.borrow();
  let mut node = frame_or_throw(ctx)?.take(ctx, &el)?;
  let out = f(ctx, &mut node);
  frame().put(&el, node);
  out
}

fn missing(ctx: &Ctx<'_>, kind: &str, what: &str) -> rquickjs::Error {
  Exception::throw_type(ctx, &format!("{kind} has no {what}"))
}

/// Applies a gpui `Styled` method to the node's style.
pub fn restyle(
  ctx: &Ctx<'_>,
  n: &mut Box<dyn Node>,
  f: impl FnOnce(StyleRefinement) -> StyleRefinement,
) -> Result<()> {
  let kind = n.kind();
  let style = n.style().ok_or_else(|| missing(ctx, kind, "style"))?;
  *style = f(mem::take(style));
  Ok(())
}

/// Applies a gpui `InteractiveElement`/`StatefulInteractiveElement` method to the node.
pub(crate) fn interact(ctx: &Ctx<'_>, n: &mut Box<dyn Node>, f: impl FnOnce(I) -> I) -> Result<()> {
  let kind = n.kind();
  let i = n
    .interactivity()
    .ok_or_else(|| missing(ctx, kind, "event handlers"))?;
  *i = f(I(mem::take(i))).0;
  Ok(())
}

/// Applies a builder method of the concrete type `T`, e.g. `Button::primary`. On another
/// type it throws a miss, which lets a same-named method of another family run instead.
pub fn map<T: Node>(ctx: &Ctx<'_>, n: &mut Box<dyn Node>, f: impl FnOnce(T) -> T) -> Result<()> {
  if !(&**n as &dyn Any).is::<T>() {
    return Err(miss(ctx, n.kind()));
  }
  let any = mem::replace(n, Box::new(StyleRefinement::default())).into_any();
  *n = Box::new(f(*any.downcast::<T>().expect("checked")));
  Ok(())
}

thread_local! {
  /// Set by [`miss`]: the method does not apply to this element
  static MISS: Cell<bool> = const { Cell::new(false) };
}

/// Throws "not available": the method does not apply to an element of `kind`. A same-named
/// method defined before it then gets its turn.
pub fn miss(ctx: &Ctx<'_>, kind: &str) -> rquickjs::Error {
  MISS.set(true);
  Exception::throw_type(ctx, &format!("this method is not available on {kind}"))
}

/// Puts method `f` on the element prototype as `name`. Several families may define one name:
/// the latest runs first and, on an element not of its `kinds` (checked before its arguments
/// are) or when it throws a [`miss`], hands the call to the one defined before it.
pub fn define<'js>(
  proto: &rquickjs::Object<'js>,
  name: &str,
  kinds: Option<&'static [&'static str]>,
  f: Function<'js>,
) -> Result<()> {
  let prev: Option<Function<'js>> = proto.get(name)?;
  if prev.is_none() && kinds.is_none() {
    return proto.set(name, f);
  }
  let dispatch = move |this: This<Class<'js, El>>, args: Rest<Value<'js>>| -> Result<Value<'js>> {
    let ctx = this.0.ctx().clone();
    let mut kind = "";
    with_node(&this.0, |_, n| {
      kind = n.kind();
      Ok(())
    })?;
    if kinds.is_none_or(|k| k.contains(&kind)) {
      MISS.set(false);
      match f.call::<_, Value>((This(this.0.clone()), Rest(args.0.clone()))) {
        Err(_) if MISS.replace(false) => drop(ctx.catch()),
        out => return out,
      }
    }
    match &prev {
      Some(prev) => prev.call((This(this.0), Rest(args.0))),
      None => Err(miss(&ctx, kind)),
    }
  };
  proto.set(name, Function::new(proto.ctx().clone(), dispatch)?)
}

/// Calls `f` with a style-only element and returns the style it built; what `f` returns is
/// ignored.
pub(crate) fn styled<'js>(ctx: &Ctx<'js>, f: Function<'js>) -> Result<StyleRefinement> {
  let style = new_el(ctx, StyleRefinement::default())?;
  f.call::<_, Value>((style.clone(),))?;
  let mut out = StyleRefinement::default();
  with_node(&style, |ctx, n| {
    restyle(ctx, n, |s| {
      out = s;
      StyleRefinement::default()
    })
  })?;
  Ok(out)
}

/// What `child` takes
pub enum Child<'js> {
  El(Class<'js, El>),
  View(Class<'js, ViewHandle>),
  Text(String),
}

impl<'js> FromJs<'js> for Child<'js> {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    if let Some(text) = value.as_string() {
      return Ok(Self::Text(text.to_string()?));
    }
    if value.is_number() || value.is_bool() {
      return Ok(Self::Text(value.get::<rquickjs::Coerced<String>>()?.0));
    }
    if let Ok(view) = Class::<ViewHandle>::from_js(ctx, value.clone()) {
      return Ok(Self::View(view));
    }
    Class::from_js(ctx, value)
      .map(Self::El)
      .map_err(|_| Exception::throw_type(ctx, "expected an element, an entity or a string"))
  }
}

/// The TypeScript type of a builder method argument
pub trait TsType {
  fn ts() -> String;
  /// Optional params end with `?`
  const OPTIONAL: bool = false;
}

macro_rules! ts {
  ($($ty:ty => $ts:literal),* $(,)?) => {
    $(impl TsType for $ty {
      fn ts() -> String {
        $ts.into()
      }
    })*
  };
}

ts! {
  f32 => "number",
  f64 => "number",
  u32 => "number",
  i32 => "number",
  bool => "boolean",
  String => "string",
  Child<'_> => "Element | Entity | string | number | boolean",
  Vec<Child<'_>> => "Array<Element | Entity | string | number | boolean>",
  Vec<String> => "string[]",
  Function<'_> => "(...args: any[]) => unknown",
}

impl<T: TsType> TsType for Opt<T> {
  fn ts() -> String {
    T::ts()
  }
  const OPTIONAL: bool = true;
}

/// A JS class an arg takes, typed by its TS name
impl<'js, T: rquickjs::class::JsClass<'js> + Named> TsType for Class<'js, T> {
  fn ts() -> String {
    T::TS.into()
  }
}

/// The TS name of a class
pub trait Named {
  const TS: &'static str;
}

/// An event payload, typed by its TS name
pub trait Payload {
  const TS: &'static str;
}

/// An event handler `(event: P, cx: Context) => void`
pub struct On<'js, P>(pub Function<'js>, PhantomData<P>);

impl<'js, P> FromJs<'js> for On<'js, P> {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    Ok(Self(Function::from_js(ctx, value)?, PhantomData))
  }
}

impl<P: Payload> TsType for On<'_, P> {
  fn ts() -> String {
    format!("(event: {}, cx: Context) => void", P::TS)
  }
}

/// A `/** */` comment from doc lines, or nothing.
pub fn jsdoc(lines: &[&str], indent: &str) -> String {
  match lines {
    [] => String::new(),
    [line] => format!("{indent}/**{line} */\n"),
    lines => {
      let body: String = lines.iter().map(|l| format!("{indent} *{l}\n")).collect();
      format!("{indent}/**\n{body}{indent} */\n")
    }
  }
}

/// One line per builder method, `fn name(args) |ctx, node| body;` with `body` a
/// `rquickjs::Result<()>` changing `node: &mut Box<dyn Node>`. A leading `kinds: KINDS;`
/// limits the table to elements of those kinds, see [`define`]. Emits `install(proto)`, which
/// puts the methods on the element prototype, and `d_ts()`, their `interface Element` members;
/// doc comments become JSDoc.
#[macro_export]
macro_rules! el_methods {
  (kinds: $kinds:expr; $($rest:tt)*) => {
    $crate::el_methods!(@table Some($kinds); $($rest)*);
  };
  (@table $kinds:expr; $(
    $(#[doc = $doc:literal])*
    fn $name:ident($($arg:ident: $ty:ty),* $(,)?) |$ctx:pat_param, $n:pat_param| $body:expr;
  )*) => {
    /// Puts these methods on the element prototype.
    pub fn install<'js>(proto: &$crate::rquickjs::Object<'js>) -> $crate::rquickjs::Result<()> {
      $(
        let f = $crate::rquickjs::Function::new(
          proto.ctx().clone(),
          |this: $crate::rquickjs::function::This<$crate::rquickjs::Class<'js, $crate::El>>,
           $($arg: $ty),*| {
            $crate::with_node(&this.0, |$ctx, $n| $body)?;
            Ok::<_, $crate::rquickjs::Error>(this.0)
          },
        )?;
        $crate::define(proto, stringify!($name), $kinds, f)?;
      )*
      Ok(())
    }

    /// These methods as members of `interface Element`
    #[allow(clippy::extra_unused_lifetimes)] // names the `'js` of the argument types
    pub fn d_ts<'js>() -> String {
      let mut out = String::new();
      $(
        out.push_str(&$crate::jsdoc(&[$($doc),*], "    "));
        let args: &[String] = &[$(format!(
          "{}{}: {}",
          stringify!($arg),
          if <$ty as $crate::TsType>::OPTIONAL { "?" } else { "" },
          <$ty as $crate::TsType>::ts(),
        )),*];
        out.push_str(&format!("    {}({}): Element;\n", stringify!($name), args.join(", ")));
      )*
      out
    }
  };
  ($($rest:tt)*) => {
    $crate::el_methods!(@table None; $($rest)*);
  };
}

/// Puts every builder method on the element prototype, once per context.
pub(crate) fn install(ctx: &Ctx<'_>) -> Result<()> {
  let proto = Class::<El>::prototype(ctx)?.expect("element prototype");
  crate::element::style::install(&proto)?;
  methods::install(&proto)?;
  crate::element::events::install(&proto)?;
  fns::install(&proto)?;
  for family in crate::element::components::FAMILIES {
    (family.methods)(&proto)?;
  }
  Ok(())
}

/// The members of `interface Element`
pub(crate) fn el_d_ts() -> String {
  let mut out =
    crate::element::style::d_ts() + &methods::d_ts() + &crate::element::events::d_ts() + fns::D_TS;
  for family in crate::element::components::FAMILIES {
    out.push_str(&(family.el_d_ts)());
  }
  out
}

/// An element id: a name or a number
pub struct Id(gpui_kit::ElementId);

impl<'js> FromJs<'js> for Id {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    match value.as_number() {
      Some(n) if n.is_finite() && n >= 0. && n.fract() == 0. => {
        Ok(Id(gpui_kit::ElementId::Integer(n as u64)))
      }
      Some(_) => Err(Exception::throw_type(
        ctx,
        "an element id number must be a non-negative integer",
      )),
      None => Ok(Id(gpui_kit::ElementId::Name(
        String::from_js(ctx, value)?.into(),
      ))),
    }
  }
}

impl TsType for Id {
  fn ts() -> String {
    "string | number".into()
  }
}

impl Id {
  pub fn into_inner(self) -> gpui_kit::ElementId {
    self.0
  }
}

mod methods {
  use gpui_kit::{KeyContext, Role};
  use rquickjs::{Class, Exception};

  use super::{Child, Id, interact};
  use crate::{FocusHandle, Opt, ScrollHandle, frame, frame_or_throw};

  /// The accessibility roles a script can name
  const ROLES: &[(&str, Role)] = &[
    ("alert", Role::Alert),
    ("button", Role::Button),
    ("checkbox", Role::CheckBox),
    ("dialog", Role::Dialog),
    ("group", Role::Group),
    ("heading", Role::Heading),
    ("image", Role::Image),
    ("link", Role::Link),
    ("list", Role::List),
    ("list_item", Role::ListItem),
    ("listbox", Role::ListBox),
    ("menu", Role::Menu),
    ("menu_item", Role::MenuItem),
    ("option", Role::ListBoxOption),
    ("progress_indicator", Role::ProgressIndicator),
    ("radio_button", Role::RadioButton),
    ("slider", Role::Slider),
    ("status", Role::Status),
    ("switch", Role::Switch),
    ("tab", Role::Tab),
    ("tab_list", Role::TabList),
    ("tab_panel", Role::TabPanel),
    ("text_input", Role::TextInput),
    ("toolbar", Role::Toolbar),
    ("tooltip", Role::Tooltip),
  ];

  crate::el_methods! {
    /// Adds one child. A string is text; an `Entity` from `cx.new` mounts a retained view.
    fn child(child: Child<'js>) |ctx, n| {
      let kind = n.kind();
      let child = frame_or_throw(ctx)?.child(ctx, child)?;
      match n.add_child(child) {
        true => Ok(()),
        false => Err(Exception::throw_type(ctx, &format!("{kind} takes no children"))),
      }
    };
    /// Adds several children, in order.
    fn children(children: Vec<Child<'js>>) |ctx, n| {
      for child in children {
        let child = frame().child(ctx, child)?;
        if !n.add_child(child) {
          return Err(Exception::throw_type(ctx, &format!("{} takes no children", n.kind())));
        }
      }
      Ok(())
    };
    /// A stable name for this element, its identity across renders.
    fn id(id: Id) |ctx, n| interact(ctx, n, |mut i| {
      i.0.element_id = Some(id.into_inner());
      i
    });
    /// The key-binding context this element and its subtree sit in.
    fn key_context(context: String) |ctx, n| {
      let context = KeyContext::parse(&context)
        .map_err(|e| Exception::throw_type(ctx, &format!("key_context: {e}")))?;
      interact(ctx, n, |i| gpui_kit::InteractiveElement::key_context(i, context))
    };
    /// Tracks a focus handle from `cx.focus_handle()`, so it can focus this element.
    fn track_focus(handle: Class<'js, FocusHandle>) |ctx, n| {
      let handle = handle.borrow().handle(ctx)?;
      interact(ctx, n, |i| gpui_kit::InteractiveElement::track_focus(i, &handle))
    };
    /// Where this element sits in the Tab order; also makes it a tab stop.
    fn tab_index(index: i32) |ctx, n| {
      interact(ctx, n, |i| gpui_kit::InteractiveElement::tab_index(i, index as isize))
    };
    /// Whether Tab can land on this element.
    fn tab_stop(value: bool) |ctx, n| {
      interact(ctx, n, |i| gpui_kit::InteractiveElement::tab_stop(i, value))
    };
    /// Lets a `ScrollHandle` read and drive this element's scroll position.
    fn track_scroll(handle: Class<'js, ScrollHandle>) |ctx, n| {
      let handle = handle.borrow().handle.clone();
      interact(ctx, n, |i| gpui_kit::StatefulInteractiveElement::track_scroll(i, &handle))
    };
    /// Scrolls both axes for overflowing children.
    fn overflow_scroll() |ctx, n| {
      interact(ctx, n, gpui_kit::StatefulInteractiveElement::overflow_scroll)
    };
    /// Scrolls horizontally for overflowing children.
    fn overflow_x_scroll() |ctx, n| {
      interact(ctx, n, gpui_kit::StatefulInteractiveElement::overflow_x_scroll)
    };
    /// Scrolls vertically for overflowing children.
    fn overflow_y_scroll() |ctx, n| {
      interact(ctx, n, gpui_kit::StatefulInteractiveElement::overflow_y_scroll)
    };
    /// The label a hover shows over this element.
    fn tooltip(text: String) |ctx, n| interact(ctx, n, |i| {
      gpui_kit::StatefulInteractiveElement::tooltip(i, move |window, cx| {
        gpui_kit::component::tooltip::Tooltip::new(text.clone()).build(window, cx)
      })
    });
    /// What this element announces itself as.
    fn role(name: String) |ctx, n| {
      let Some((_, role)) = ROLES.iter().find(|(r, _)| *r == name) else {
        let names: Vec<&str> = ROLES.iter().map(|(r, _)| *r).collect();
        return Err(Exception::throw_type(ctx, &crate::element::style::unknown("role", &name, &names)));
      };
      interact(ctx, n, |i| gpui_kit::StatefulInteractiveElement::role(i, *role))
    };
    /// What a screen reader announces.
    fn aria_label(label: String) |ctx, n| {
      interact(ctx, n, |i| gpui_kit::StatefulInteractiveElement::aria_label(i, label))
    };
    /// Same as `aria_label`.
    fn accessibility_label(label: String) |ctx, n| {
      interact(ctx, n, |i| gpui_kit::StatefulInteractiveElement::aria_label(i, label))
    };
    /// Supplementary text a screen reader announces after the label.
    fn aria_description(text: String) |ctx, n| {
      interact(ctx, n, |i| gpui_kit::StatefulInteractiveElement::aria_description(i, text))
    };
    /// The selected state, announced.
    fn aria_selected(value: bool) |ctx, n| {
      interact(ctx, n, |i| gpui_kit::StatefulInteractiveElement::aria_selected(i, value))
    };
    /// The expanded state, announced.
    fn aria_expanded(value: bool) |ctx, n| {
      interact(ctx, n, |i| gpui_kit::StatefulInteractiveElement::aria_expanded(i, value))
    };
    /// The heading level, announced.
    fn aria_level(level: u32) |ctx, n| {
      interact(ctx, n, |i| gpui_kit::StatefulInteractiveElement::aria_level(i, level as usize))
    };
    /// Announces this element as focused while an ancestor holds the keyboard.
    fn aria_active_descendant() |ctx, n| {
      interact(ctx, n, gpui_kit::StatefulInteractiveElement::aria_active_descendant)
    };
    /// Makes the element focusable without tracking a handle.
    fn focusable(value: Opt<bool>) |ctx, n| match value.0 {
      Some(false) => Ok(()),
      _ => interact(ctx, n, gpui_kit::StatefulInteractiveElement::focusable),
    };
    /// Blocks the mouse from reaching elements behind this one.
    fn occlude() |ctx, n| interact(ctx, n, gpui_kit::InteractiveElement::occlude);
    /// Names a group for `group_hover` of descendants.
    fn group(name: String) |ctx, n| {
      interact(ctx, n, |i| gpui_kit::InteractiveElement::group(i, name))
    };
  }
}

/// Methods whose shape no table line has: callbacks returning values, style callbacks
mod fns {
  use gpui_kit::StyleRefinement;
  use rquickjs::{Class, Function, Object, Result, Value, function::This};

  use super::{El, interact, with_node};

  pub const D_TS: &str = r#"    /** Passes this element to `transform` and returns what it returns. */
    map<T>(transform: (element: Element) => T): T;
    /** Applies `branch` only when `condition` is truthy. */
    when(condition: unknown, branch: (element: Element) => Element): Element;
    /** Applies `branch` only when `value` is not null or undefined, passing it along. */
    when_some<T>(value: T | null | undefined, branch: (element: Element, value: T) => Element): Element;
    /** Styles while hovered. */
    hover(style: (style: Element) => Element): Element;
    /** Styles while pressed. */
    active(style: (style: Element) => Element): Element;
    /** Styles while focused. */
    focus(style: (style: Element) => Element): Element;
    /** Styles while something inside is focused. */
    in_focus(style: (style: Element) => Element): Element;
    /** Styles while focused from the keyboard. */
    focus_visible(style: (style: Element) => Element): Element;
    /** Styles while an ancestor with `group(name)` is hovered. */
    group_hover(group: string, style: (style: Element) => Element): Element;
"#;

  fn styled<'js>(this: &Class<'js, El>, f: Function<'js>) -> Result<StyleRefinement> {
    super::styled(this.ctx(), f)
  }

  pub fn install<'js>(proto: &Object<'js>) -> Result<()> {
    let ctx = proto.ctx().clone();
    proto.set(
      "map",
      Function::new(ctx.clone(), |this: This<Value<'js>>, f: Function<'js>| {
        f.call::<_, Value>((this.0,))
      })?,
    )?;
    proto.set(
      "when",
      Function::new(
        ctx.clone(),
        |this: This<Value<'js>>, condition: rquickjs::Coerced<bool>, f: Function<'js>| {
          match condition.0 {
            true => f.call::<_, Value>((this.0,)),
            false => Ok(this.0),
          }
        },
      )?,
    )?;
    proto.set(
      "when_some",
      Function::new(
        ctx.clone(),
        |this: This<Value<'js>>, value: Value<'js>, f: Function<'js>| match value.is_null()
          || value.is_undefined()
        {
          true => Ok(this.0),
          false => f.call::<_, Value>((this.0, value)),
        },
      )?,
    )?;
    macro_rules! state_style {
      ($($name:ident),*) => {$(
        proto.set(
          stringify!($name),
          Function::new(ctx.clone(), |this: This<Class<'js, El>>, f: Function<'js>| {
            let style = styled(&this.0, f)?;
            with_node(&this.0, |ctx, n| interact(ctx, n, |i| {
              gpui_kit::InteractiveElement::$name(i, |_| style)
            }))?;
            Ok::<_, rquickjs::Error>(this.0)
          })?,
        )?;
      )*};
    }
    state_style!(hover, focus, in_focus, focus_visible);
    proto.set(
      "active",
      Function::new(
        ctx.clone(),
        |this: This<Class<'js, El>>, f: Function<'js>| {
          let style = styled(&this.0, f)?;
          with_node(&this.0, |ctx, n| {
            interact(ctx, n, |i| {
              gpui_kit::StatefulInteractiveElement::active(i, |_| style)
            })
          })?;
          Ok::<_, rquickjs::Error>(this.0)
        },
      )?,
    )?;
    proto.set(
      "group_hover",
      Function::new(
        ctx.clone(),
        |this: This<Class<'js, El>>, group: String, f: Function<'js>| {
          let style = styled(&this.0, f)?;
          with_node(&this.0, |ctx, n| {
            interact(ctx, n, |i| {
              gpui_kit::InteractiveElement::group_hover(i, group, |_| style)
            })
          })?;
          Ok::<_, rquickjs::Error>(this.0)
        },
      )?,
    )?;
    Ok(())
  }
}
