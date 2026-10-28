//! Builder methods: `div().flex().child(..)` on a Rust-arena [`El`], one table line each.

use gpui_kit::{
  AnyElement, Div, InteractiveElement, IntoElement, ParentElement, Stateful,
  StatefulInteractiveElement, Styled, WeakEntity, div, px, rgb,
};
use rquickjs::{
  Class, Ctx, Exception, FromJs, Function, JsLifetime, Persistent, Result, Value, class::Trace,
  function::This,
};

use crate::{ScriptView, frame};

/// The elements one render builds, and the click handlers they hold
pub struct Frame {
  nodes: Vec<Option<Stateful<Div>>>,
  pub(crate) handlers: Vec<Persistent<Function<'static>>>,
  view: WeakEntity<ScriptView>,
  #[cfg(test)]
  pub(crate) text: String,
}

impl Frame {
  pub(crate) fn new(view: WeakEntity<ScriptView>) -> Self {
    Self {
      nodes: Vec::new(),
      handlers: Vec::new(),
      view,
      #[cfg(test)]
      text: String::new(),
    }
  }

  fn push(&mut self) -> usize {
    self.nodes.push(Some(div().id(self.nodes.len())));
    self.nodes.len() - 1
  }

  /// Moves element `id` out, once.
  pub(crate) fn take(&mut self, ctx: &Ctx<'_>, id: usize) -> Result<Stateful<Div>> {
    match self.nodes.get_mut(id).and_then(Option::take) {
      Some(el) => Ok(el),
      None => Err(Exception::throw_message(
        ctx,
        "element was already used as a child",
      )),
    }
  }

  fn child(&mut self, child: Child<'_>) -> Result<AnyElement> {
    Ok(match child {
      Child::El(el) => {
        let id = el.borrow().id;
        self.take(el.ctx(), id)?.into_any_element()
      }
      Child::Text(text) => {
        #[cfg(test)]
        self.text.push_str(&text);
        text.into_any_element()
      }
    })
  }

  fn on_click(&mut self, el: Stateful<Div>, handler: Function<'_>) -> Stateful<Div> {
    let index = self.handlers.len();
    self
      .handlers
      .push(Persistent::save(&handler.ctx().clone(), handler));
    let view = self.view.clone();
    el.on_click(move |_, window, cx| {
      if let Some(view) = view.upgrade() {
        view.update(cx, |view, cx| view.dispatch(index, window, cx));
      }
    })
  }
}

/// An element under construction; its gpui element lives in the [`Frame`].
#[derive(Trace, JsLifetime)]
#[rquickjs::class]
pub struct El {
  #[qjs(skip_trace)]
  pub(crate) id: usize,
}

/// A new element
pub(crate) fn new_el(ctx: Ctx<'_>) -> Result<Class<'_, El>> {
  Class::instance(ctx, El { id: frame().push() })
}

/// What `child` takes
pub enum Child<'js> {
  El(Class<'js, El>),
  Text(String),
}

impl<'js> FromJs<'js> for Child<'js> {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    match value.as_string() {
      Some(text) => Ok(Self::Text(text.to_string()?)),
      None => Ok(Self::El(Class::from_js(ctx, value)?)),
    }
  }
}

/// The TypeScript type of a builder method argument
pub trait TsType {
  const TS: &'static str;
}

impl TsType for u32 {
  const TS: &'static str = "number";
}
impl TsType for f32 {
  const TS: &'static str = "number";
}
impl TsType for Child<'_> {
  const TS: &'static str = "El | string";
}
impl TsType for Function<'_> {
  const TS: &'static str = "(event: unknown, cx: ViewCx) => void";
}

/// One line per builder method: `fn name(args) |el, frame| new_el;`. Emits the
/// `#[rquickjs::methods]` impl on [`El`] and [`el_d_ts`], doc comments become JSDoc.
macro_rules! gpui_el {
  ($(
    $(#[doc = $doc:literal])*
    fn $name:ident($($arg:ident: $ty:ty),*) |$el:ident, $frame:ident| $body:expr;
  )*) => {
    #[rquickjs::methods]
    impl El {
      $(
        $(#[doc = $doc])*
        pub fn $name<'js>(this: This<Class<'js, El>>, $($arg: $ty),*) -> Result<Class<'js, El>> {
          let $frame = frame();
          let $el = $frame.take(this.ctx(), this.borrow().id)?;
          let el = $body;
          $frame.nodes[this.borrow().id] = Some(el);
          Ok(this.0)
        }
      )*
    }

    /// The `El` interface
    #[allow(clippy::extra_unused_lifetimes)] // names the `'js` of the argument types
    pub fn el_d_ts<'js>() -> String {
      let mut out = String::from("export interface El {\n");
      $(
        out.push_str(&jsdoc(&[$($doc),*], "  "));
        let args: &[String] = &[$(format!("{}: {}", stringify!($arg), <$ty as TsType>::TS)),*];
        out.push_str(&format!("  {}({}): El;\n", stringify!($name), args.join(", ")));
      )*
      out.push_str("}\n");
      out
    }
  };
}

/// A `/** */` comment from doc lines, or nothing.
pub(crate) fn jsdoc(lines: &[&str], indent: &str) -> String {
  match lines {
    [] => String::new(),
    [line] => format!("{indent}/**{line} */\n"),
    lines => {
      let body: String = lines.iter().map(|l| format!("{indent} *{l}\n")).collect();
      format!("{indent}/**\n{body}{indent} */\n")
    }
  }
}

gpui_el! {
  /// Lays the children out with flexbox.
  fn flex() |el, _f| el.flex();
  /// Background color as `0xRRGGBB`.
  fn bg(color: u32) |el, _f| el.bg(rgb(color));
  /// Space between the children, in pixels.
  fn gap(size: f32) |el, _f| el.gap(px(size));
  /// Fills the parent.
  fn size_full() |el, _f| el.size_full();
  /// Appends an element or a text.
  fn child(child: Child<'js>) |el, f| el.child(f.child(child)?);
  /// Calls `handler` when the element is clicked.
  fn on_click(handler: Function<'js>) |el, f| f.on_click(el, handler);
}
