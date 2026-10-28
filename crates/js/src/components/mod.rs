//! Component bindings: gpui-component (and gpui-base) widgets as script elements, one file
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
//!    Bodies reach the concrete type through [`map`](crate::map), which throws on other
//!    elements: `fn primary() |c, n| crate::map(c, n, |b: Button| b.primary());`. Use
//!    [`On`](crate::On) for handlers (`frame().listener(..)` makes the gpui listener) and the
//!    value types of `style` (`Color`, `Len`, ..) for arguments.
//! 5. Retained states (`InputState`, ..) are rquickjs classes holding a
//!    [`Retained`](crate::Retained) with `on`/`release` methods; `Named` gives them a TS name.
//! 6. Fill [`Family`]: the module (`"gpui-component"` or `"gpui-base"`), the export names,
//!    `exports` making their values, `methods: install` and the d.ts of both. Then run the
//!    `d_ts_unchanged` test and update `src/tests.d.ts`.
//!
//! ```ignore
//! pub(crate) const FAMILY: Family = Family {
//!   module: "gpui-component",
//!   names: &["Button"],
//!   exports: |ctx| Ok(vec![("Button", constructor(ctx, "Button", Function::new(ctx.clone(), button)?)?)]),
//!   methods: install,
//!   d_ts: || "  export const Button: { new (id: string): Element; new: (id: string) => Element };\n".into(),
//!   el_d_ts: d_ts,
//! };
//! ```

use rquickjs::{Ctx, Function, Object, Result, Value};

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

/// Every family, one line each
pub(crate) const FAMILIES: &[Family] = &[
  #[cfg(test)]
  crate::tests::FAMILY,
];

/// A JS constructor for `make`: `new Name(..)` and `Name.new(..)` both return what it does.
pub fn constructor<'js>(ctx: &Ctx<'js>, name: &str, make: Function<'js>) -> Result<Value<'js>> {
  let wrap: Function = ctx.eval(format!(
    "(make) => {{ function {name}(...args) {{ return make(...args); }} {name}.new = make; return {name}; }}"
  ))?;
  wrap.call((make,))
}
