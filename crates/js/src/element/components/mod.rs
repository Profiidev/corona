//! Component bindings: `corona/component` (and `corona/base`) widgets as script elements, one file
//! per family, each registered by one line in [`FAMILIES`].
//!
//! # Adding a family
//!
//! 1. Create `components/<family>.rs` and add `mod <family>;` below plus
//!    `<family>::FAMILY,` to [`FAMILIES`]. Nothing else outside the file changes.
//! 2. Make each widget a [`Node`](crate::Node): `crate::node!(Button, "Button", style,
//!    interactive, parent);`, naming the gpui traits it implements (`Styled`,
//!    `InteractiveElement`, `ParentElement`). Style, event and child methods then work on it.
//! 3. Export constructors with [`constructor`]: `new Button(id)` and `Button.new(id)` both
//!    return an element, made by `crate::new_el(ctx, Button::new(id.into_inner()))`.
//! 4. Add the widget's own builder methods with an [`el_methods!`](crate::el_methods) table.
//!    Each line names the kinds it works on, `fn label(text: String) for ["Button", "Checkbox"]
//!    |c, n| ..`, which the d.ts types it on (`this: Element<"Button" | "Checkbox">`) and the
//!    runtime checks before the arguments. Bodies reach the concrete type through
//!    [`map`](crate::map) or `pick!(c, n, Button => |b: Button| b.primary(), ..)`, which throw a
//!    [`miss`](crate::miss) on other elements. Several families (and the style and event
//!    tables) may define one name: [`define`](crate::define) tries the latest first and hands
//!    an element outside its kinds, or a miss, to the one before. So check the kind before
//!    taking a child out of the frame. Use [`On`](crate::On) for handlers ([`listen`] makes
//!    the gpui listener), `choice!` for arguments naming one of a few values and the value
//!    types of `style` (`Color`, `Len`, ..) for the rest. Add the kind to `element/caps.rs`
//!    with what it takes (style, interaction, children), and type its constructor
//!    `Element<"Kind">` in the d.ts.
//! 5. Retained states (`InputState`, ..) are rquickjs classes holding a
//!    [`Retained`](crate::Retained) with `on`/`release` methods; `Named` gives them a TS name.
//! 6. Fill [`Family`]: the module (`"corona/component"` or `"corona/base"`), the export names,
//!    `exports` making their values, `methods: install` and the d.ts of both. Then run the
//!    `d_ts_unchanged` test and update `src/tests/kit.d.ts`.
//!
//! ```ignore
//! pub(crate) const FAMILY: Family = Family {
//!   module: "corona/component",
//!   names: &["Button"],
//!   exports: |ctx| Ok(vec![("Button", constructor(ctx, "Button", Function::new(ctx.clone(), button)?)?)]),
//!   methods: install,
//!   d_ts: || "  export const Button: { new (id: string): Element; new: (id: string) => Element };\n".into(),
//!   el_d_ts: d_ts,
//! };
//! ```

use std::{cell::RefCell, rc::Rc};

use gpui_kit::{App, Entity, EventEmitter, Window, component::Size};
use rquickjs::{Class, Ctx, Exception, Function, IntoJs, Object, Result, Value};

use crate::{Child, El, Node, frame, frame_or_throw, runtime::runtime, view::window_or_throw};

/// `choice! { Sz("size"): Size { "small" => Size::Small, .. } }`: an argument naming one of a
/// few values, holding the value it names
macro_rules! choice {
  ($($ty:ident($what:literal): $out:ty { $($name:literal => $value:expr),* $(,)? })*) => {$(
    pub(crate) struct $ty(pub $out);

    impl<'js> rquickjs::FromJs<'js> for $ty {
      fn from_js(ctx: &rquickjs::Ctx<'js>, value: rquickjs::Value<'js>) -> rquickjs::Result<Self> {
        let name = String::from_js(ctx, value)?;
        match name.as_str() {
          $($name => Ok(Self($value)),)*
          _ => Err(rquickjs::Exception::throw_type(
            ctx,
            &crate::element::style::unknown($what, &name, &[$($name),*]),
          )),
        }
      }
    }

    impl crate::TsType for $ty {
      fn ts() -> String {
        [$(concat!("\"", $name, "\"")),*].join(" | ")
      }
    }
  )*};
}

/// `pick!(c, n, Button => |b: Button| b.primary(), ..)`: applies the closure of the node's
/// type, a [`miss`](crate::miss) on any other
macro_rules! pick {
  ($c:expr, $n:expr, $($ty:ty => $f:expr),+ $(,)?) => {{
    let (c, n) = ($c, $n);
    $(if $crate::element::components::is::<$ty>(&**n) { $crate::map(c, n, $f) } else)+ {
      Err($crate::miss(c, n.kind()))
    }
  }};
}

/// A retained state's JS class: holds a [`Retained`](crate::Retained), typed by its JS name
macro_rules! state {
  ($ty:ident, $state:ty, $js:literal) => {
    #[derive(rquickjs::class::Trace, rquickjs::JsLifetime)]
    #[rquickjs::class(rename = $js)]
    pub struct $ty {
      #[qjs(skip_trace)]
      pub(crate) state: crate::Retained<$state>,
    }

    impl crate::Named for $ty {
      const TS: &'static str = $js;
    }

    impl $ty {
      #[allow(dead_code)]
      pub(crate) fn instance<'js>(
        ctx: &rquickjs::Ctx<'js>,
        state: gpui_kit::Entity<$state>,
      ) -> rquickjs::Result<rquickjs::Class<'js, Self>> {
        let state = crate::Retained::new(ctx, state)?;
        rquickjs::Class::instance(ctx.clone(), Self { state })
      }

      /// The entity of this state, or a JS error once released
      #[allow(dead_code)]
      pub(crate) fn get(
        this: &rquickjs::Class<'_, Self>,
      ) -> rquickjs::Result<gpui_kit::Entity<$state>> {
        let ctx = this.ctx().clone();
        this.borrow().state.entity(&ctx).cloned()
      }
    }
  };
}

/// Makes a family's exports, by name
pub(crate) type Exports = for<'js> fn(&Ctx<'js>) -> Result<Vec<(&'static str, Value<'js>)>>;

/// A family of component bindings
pub(crate) struct Family {
  /// The module its constructors are imported from
  pub module: &'static str,
  /// What the module exports for it
  pub names: &'static [&'static str],
  /// Makes the exports, by name
  pub exports: Exports,
  /// Puts its element methods on the prototype, `install` of its `el_methods!`
  pub methods: for<'js> fn(&Object<'js>) -> Result<()>,
  /// Its declarations inside `declare module "<module>"`
  pub d_ts: fn() -> String,
  /// Its members of `interface Element`, `d_ts` of its `el_methods!`
  pub el_d_ts: fn() -> String,
}

mod data;
mod dock;
pub(crate) mod forms;
pub(crate) mod misc;
mod navigation;
mod overlays;

/// Every family, one line each
pub(crate) const FAMILIES: &[Family] = &[
  #[cfg(test)]
  crate::tests::FAMILY,
  forms::FAMILY,
  forms::BASE,
  navigation::FAMILY,
  navigation::BASE,
  overlays::FAMILY,
  overlays::BASE,
  data::FAMILY,
  data::BASE,
  misc::FAMILY,
  misc::BASE,
  misc::FPS,
  dock::FAMILY,
];

/// A JS constructor for `make`: `new Name(..)` and `Name.new(..)` both return what it does.
pub fn constructor<'js>(ctx: &Ctx<'js>, name: &str, make: Function<'js>) -> Result<Value<'js>> {
  let wrap: Function = ctx.eval(format!(
    "(make) => {{ function {name}(...args) {{ return make(...args); }} {name}.new = make; return {name}; }}"
  ))?;
  wrap.call((make,))
}

/// Whether the node is a `T`
pub(crate) fn is<T: 'static>(n: &dyn Node) -> bool {
  (n as &dyn std::any::Any).is::<T>()
}

/// Moves a child element out of the frame as the widget `T`, or throws `what`.
pub(crate) fn take<T: 'static>(c: &Ctx<'_>, child: Child<'_>, what: &str) -> Result<T> {
  let Child::El(el) = child else {
    return Err(Exception::throw_type(c, what));
  };
  take_el(c, &el, what)
}

/// Moves element `el` out of the frame as the widget `T`, or throws `what`.
pub(crate) fn take_el<T: 'static>(c: &Ctx<'_>, el: &Class<'_, El>, what: &str) -> Result<T> {
  let node = frame_or_throw(c)?.take(c, &el.borrow())?;
  match node.into_any().downcast::<T>() {
    Ok(t) => Ok(*t),
    Err(_) => Err(Exception::throw_type(c, what)),
  }
}

/// A constructor export, see [`constructor`]
pub(crate) fn ctor<'js, P: 'js>(
  ctx: &Ctx<'js>,
  name: &'static str,
  f: impl rquickjs::function::IntoJsFunc<'js, P> + 'js,
) -> Result<(&'static str, Value<'js>)> {
  Ok((
    name,
    constructor(ctx, name, Function::new(ctx.clone(), f)?)?,
  ))
}

/// The declaration of a constructor export: `new Name(args)` and `Name.new(args)`
pub(crate) fn declare(doc: &str, name: &str, args: &str) -> String {
  let doc = crate::jsdoc(&[&format!(" {doc}")], "  ");
  format!("{doc}  export const {name}: {{ new ({args}): Element; new: ({args}) => Element }};\n")
}

choice! {
  Sz("size"): Size {
    "xsmall" => Size::XSmall,
    "small" => Size::Small,
    "medium" => Size::Medium,
    "large" => Size::Large,
  }
}

/// A component size, or a style length on an element without one: `size(..)` is both
pub(crate) struct SizeArg<'js>(Value<'js>);

impl<'js> rquickjs::FromJs<'js> for SizeArg<'js> {
  fn from_js(_: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    Ok(Self(value))
  }
}

impl crate::TsType for SizeArg<'_> {
  fn ts() -> String {
    <Sz as crate::TsType>::ts()
  }
}

impl<'js> SizeArg<'js> {
  /// The size, a miss for a length so the style method sizes the element
  pub(crate) fn get(self, c: &Ctx<'js>, n: &dyn Node) -> Result<Size> {
    match self.0.is_string() {
      true => Ok(<Sz as rquickjs::FromJs>::from_js(c, self.0)?.0),
      false => Err(crate::miss(c, n.kind())),
    }
  }
}

/// A gpui listener taking `P`
type Listener<P> = Rc<dyn Fn(&P, &mut Window, &mut App)>;

/// The listener the latest render of the view gave, with the slot and frame generation
/// whose handlers its index points into
type Latest<P> = Rc<RefCell<Option<(u64, u32, Listener<P>)>>>;

/// `handler(payload, cx)` for the events of `entity` that `pick` maps, as an element method
/// sets it: subscribed once per entity and `key`, and calling the handler of the latest render.
/// A render that stops setting it leaves the last listener, which is ignored once its slot
/// rendered again.
pub(crate) fn on_entity<'js, T, E, P>(
  ctx: &Ctx<'js>,
  entity: &Entity<T>,
  key: &'static str,
  handler: Function<'js>,
  pick: impl Fn(&Entity<T>, &E, &App) -> Option<P> + 'static,
) -> Result<()>
where
  T: EventEmitter<E>,
  E: 'static,
  P: for<'a> IntoJs<'a> + Clone + 'static,
{
  let mut frame = frame_or_throw(ctx)?;
  let view = frame.view.entity_id();
  let listener: Listener<P> = Rc::new(frame.listener(handler, |p: &P| p.clone()));
  let rt = runtime(ctx)?;
  let mut fresh = false;
  let id = format!("{key}:{}", entity.entity_id());
  let latest: Latest<P> = rt.keyed(view, id.clone(), || {
    fresh = true;
    Rc::default()
  });
  *latest.borrow_mut() = Some((frame.slot, frame.generation, listener));
  if fresh {
    let mut window = window_or_throw(ctx)?;
    let weak = Rc::downgrade(&rt);
    let sub = window.subscribe(
      entity,
      &mut crate::app(),
      move |entity, event, window, cx| {
        let Some(payload) = pick(&entity, event, cx) else {
          return;
        };
        let latest = latest.borrow().clone();
        let Some((slot, generation, listener)) = latest else {
          return;
        };
        let current = weak.upgrade().is_some_and(|rt| {
          (rt.slots.borrow())
            .get(&(view, slot))
            .is_some_and(|s| s.generation == generation)
        });
        if current {
          listener(&payload, window, cx);
        }
      },
    );
    rt.set_keyed(view, format!("{id}:sub"), sub);
  }
  Ok(())
}

/// The handler a component takes for `event`, kept for this render
pub(crate) fn listen<
  P: for<'a> IntoJs<'a> + 'static,
  E: ?Sized + 'static,
  Q: Fn(&E) -> P + 'static,
>(
  handler: Function<'_>,
  payload: Q,
) -> impl Fn(&E, &mut Window, &mut App) + 'static + use<P, E, Q> {
  frame().listener(handler, payload)
}
