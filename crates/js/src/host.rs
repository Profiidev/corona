//! Host functions: plain `#[specta]` fns, bound by `rquickjs::Func::from` and typed by specta.

use std::{
  fmt::Display,
  ops::{Deref, DerefMut},
};

use gpui_kit::App;
use rquickjs::{
  Ctx, Exception, IntoJs, Result, Value,
  function::{FromParam, ParamRequirement, ParamsAccessor},
};
use specta::{
  Types,
  datatype::{DataType, Function, Primitive},
  function::{FunctionArg, FunctionResult},
};
use specta_typescript::{Typescript, primitives};

use crate::{app, el::jsdoc};

/// The app, injected into a host fn without taking a JS argument. Usable in sync fns and
/// in the polls of async ones, which run in the runtime driver.
pub struct Cx;

impl Deref for Cx {
  type Target = App;
  fn deref(&self) -> &App {
    app()
  }
}

impl DerefMut for Cx {
  fn deref_mut(&mut self) -> &mut App {
    app()
  }
}

impl<'js> FromParam<'js> for Cx {
  fn param_requirement() -> ParamRequirement {
    ParamRequirement::none()
  }

  fn from_param<'a>(_: &mut ParamsAccessor<'a, 'js>) -> Result<Self> {
    Ok(Cx)
  }
}

impl FunctionArg for Cx {
  fn to_datatype(_: &mut Types) -> Option<DataType> {
    None
  }
}

/// Returns `T` or throws an `Error` with the message. Typed as `T`.
pub struct Throws<T>(pub std::result::Result<T, String>);

impl<T, E: Display> From<std::result::Result<T, E>> for Throws<T> {
  fn from(result: std::result::Result<T, E>) -> Self {
    Self(result.map_err(|e| e.to_string()))
  }
}

impl<'js, T: IntoJs<'js>> IntoJs<'js> for Throws<T> {
  fn into_js(self, ctx: &Ctx<'js>) -> Result<Value<'js>> {
    match self.0 {
      Ok(value) => value.into_js(ctx),
      Err(message) => Err(Exception::throw_message(ctx, &message)),
    }
  }
}

#[doc(hidden)]
pub enum ThrowsMarker {}

impl<T: specta::Type> FunctionResult<ThrowsMarker> for Throws<T> {
  fn to_datatype(types: &mut Types) -> DataType {
    T::definition(types)
  }
}

/// `declare module "name" { .. }` with one `export function` per host fn.
pub fn render_module(name: &str, types: &Types, fns: &[Function]) -> String {
  let ts = Typescript::default();
  // specta-typescript writes floats as `number | null` (serde turns NaN into null), but
  // rquickjs passes them as numbers. ponytail: top level only, nested ones need a `Format`
  let ty = |dt: &DataType| match dt {
    DataType::Primitive(Primitive::f16 | Primitive::f32 | Primitive::f64) => "number".into(),
    dt => primitives::reference(&ts, types, dt).expect("host fn type"),
  };
  let mut out = format!("declare module \"{name}\" {{\n");
  for f in fns {
    let docs: Vec<&str> = f.docs.lines().map(|l| l.trim_end()).collect();
    out.push_str(&jsdoc(&docs, "  "));
    let args: Vec<String> = f
      .args
      .iter()
      .map(|(name, dt)| format!("{name}: {}", ty(dt)))
      .collect();
    let mut result = f.result.as_ref().map_or("void".into(), ty);
    if f.asyncness {
      result = format!("Promise<{result}>");
    }
    out.push_str(&format!(
      "  export function {}({}): {result};\n",
      f.name,
      args.join(", ")
    ));
  }
  out.push_str("}\n");
  out
}

/// A native ES module of `#[specta]` host fns:
/// `host_module!(pub Calc = "calc" { sync: [add], async: [fetch] })`.
/// Generates `Calc::install` (registers the module on a runtime's loader) and `Calc::d_ts`.
#[macro_export]
macro_rules! host_module {
  ($vis:vis $module:ident = $name:literal { sync: [$($sync:ident),* $(,)?], async: [$($async:ident),* $(,)?] $(,)? }) => {
    $vis struct $module;

    impl $module {
      pub fn install(
        resolver: &mut $crate::rquickjs::loader::BuiltinResolver,
        loader: &mut $crate::rquickjs::loader::ModuleLoader,
      ) {
        resolver.add_module($name);
        loader.add_module($name, $module);
      }

      pub fn d_ts() -> String {
        let mut types = $crate::specta::Types::default();
        let fns = [
          $($crate::specta::function::fn_datatype!($sync)(&mut types),)*
          $($crate::specta::function::fn_datatype!($async)(&mut types),)*
        ];
        $crate::render_module($name, &types, &fns)
      }
    }

    impl $crate::rquickjs::module::ModuleDef for $module {
      fn declare<'js>(decl: &$crate::rquickjs::module::Declarations<'js>) -> $crate::rquickjs::Result<()> {
        $(decl.declare(stringify!($sync))?;)*
        $(decl.declare(stringify!($async))?;)*
        Ok(())
      }

      fn evaluate<'js>(
        _ctx: &$crate::rquickjs::Ctx<'js>,
        exports: &$crate::rquickjs::module::Exports<'js>,
      ) -> $crate::rquickjs::Result<()> {
        $(exports.export(stringify!($sync), $crate::rquickjs::prelude::Func::from($sync))?;)*
        $(exports.export(stringify!($async), $crate::rquickjs::prelude::Func::from($crate::rquickjs::prelude::Async($async)))?;)*
        Ok(())
      }
    }
  };
}
