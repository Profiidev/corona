//! Host functions: plain `#[specta]` fns, bound by `rquickjs::Func::from` and typed by specta.

use std::{
  any::type_name,
  fmt::Display,
  marker::PhantomData,
  ops::{Deref, DerefMut},
};

use gpui_kit::{App, Global};
use rquickjs::{
  Ctx, Exception, FromJs, IntoJs, Result, Value,
  function::{FromParam, ParamRequirement, ParamsAccessor},
};
use specta::{
  Type, Types,
  datatype::{DataType, Fields, Primitive},
  function::FunctionArg,
};

use crate::app;

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

/// A gpui global, injected like [`Cx`]. The call throws when it is not set.
pub struct Glob<G>(PhantomData<G>);

impl<G: Global> Deref for Glob<G> {
  type Target = G;
  fn deref(&self) -> &G {
    app().global::<G>()
  }
}

impl<'js, G: Global> FromParam<'js> for Glob<G> {
  fn param_requirement() -> ParamRequirement {
    ParamRequirement::none()
  }

  fn from_param<'a>(params: &mut ParamsAccessor<'a, 'js>) -> Result<Self> {
    match app().has_global::<G>() {
      true => Ok(Glob(PhantomData)),
      false => Err(Exception::throw_message(
        params.ctx(),
        &format!("global {} not set", type_name::<G>()),
      )),
    }
  }
}

impl<G> FunctionArg for Glob<G> {
  fn to_datatype(_: &mut Types) -> Option<DataType> {
    None
  }
}

/// A param the script may leave out, `null` when it does. Typed `name?: T | null` when no
/// required param follows.
pub struct Opt<T>(pub Option<T>);

impl<'js, T: FromJs<'js>> FromParam<'js> for Opt<T> {
  fn param_requirement() -> ParamRequirement {
    ParamRequirement::optional()
  }

  fn from_param<'a>(params: &mut ParamsAccessor<'a, 'js>) -> Result<Self> {
    if params.is_empty() {
      return Ok(Opt(None));
    }
    let ctx = params.ctx().clone();
    Ok(Opt(Option::from_js(&ctx, params.arg())?))
  }
}

impl<T: Type> Type for Opt<T> {
  fn definition(types: &mut Types) -> DataType {
    Option::<T>::definition(types)
  }
}

/// Returns `T` or throws an `Error` with the message, also from an async fn. Typed as `T`.
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

impl<T: Type> Type for Throws<T> {
  fn definition(types: &mut Types) -> DataType {
    T::definition(types)
  }
}

/// Registers `#[specta]` fns on a [`ModuleBuilder`](crate::ModuleBuilder):
/// `host_module!(m, { sync: [add], async: [fetch], declare: [other] })`. Their JS names are
/// camelCase. `declare` types a fn without exporting it, for one another variant of the
/// module has: importing it fails.
#[macro_export]
macro_rules! host_module {
  ($m:expr, {
    $(sync: [$($sync:ident),* $(,)?] $(,)?)?
    $(async: [$($async:ident),* $(,)?] $(,)?)?
    $(declare: [$($declare:ident),* $(,)?] $(,)?)?
  }) => {{
    let m: &mut $crate::ModuleBuilder = $m;
    $($(
      m.func($crate::specta::function::fn_datatype!($sync), |ctx| {
        $crate::rquickjs::IntoJs::into_js($crate::rquickjs::prelude::Func::from($sync), ctx)
      });
    )*)?
    $($(
      m.func($crate::specta::function::fn_datatype!($async), |ctx| {
        let f = $crate::rquickjs::prelude::Func::from($crate::rquickjs::prelude::Async($async));
        $crate::rquickjs::IntoJs::into_js(f, ctx)
      });
    )*)?
    $($(m.declare($crate::specta::function::fn_datatype!($declare));)*)?
  }};
}

/// specta-typescript writes floats as `number | null` (serde turns NaN into null), but rquickjs
/// passes them as numbers: makes them `i32`, also written `number`. ponytail: generic
/// arguments of references stay as they are, map them too if a generic host type takes floats
pub(crate) fn floats(dt: &mut DataType) {
  match dt {
    DataType::Primitive(p @ (Primitive::f16 | Primitive::f32 | Primitive::f64)) => {
      *p = Primitive::i32
    }
    DataType::Primitive(_) | DataType::Generic(_) | DataType::Reference(_) => {}
    DataType::List(list) => floats(&mut list.ty),
    DataType::Map(map) => {
      floats(map.key_ty_mut());
      floats(map.value_ty_mut());
    }
    DataType::Struct(s) => fields(&mut s.fields),
    DataType::Enum(e) => e
      .variants
      .iter_mut()
      .for_each(|(_, v)| fields(&mut v.fields)),
    DataType::Tuple(t) => t.elements.iter_mut().for_each(floats),
    DataType::Nullable(dt) => floats(dt),
    DataType::Intersection(dts) => dts.iter_mut().for_each(floats),
  }
}

fn fields(fields: &mut Fields) {
  let tys: Vec<&mut Option<DataType>> = match fields {
    Fields::Unit => return,
    Fields::Unnamed(f) => f.fields.iter_mut().map(|f| &mut f.ty).collect(),
    Fields::Named(f) => f.fields.iter_mut().map(|(_, f)| &mut f.ty).collect(),
  };
  tys.into_iter().flatten().for_each(floats);
}

/// `snake_case` to `camelCase`
pub(crate) fn camel(name: &str) -> String {
  let mut parts = name.split('_');
  let mut out = parts.next().unwrap_or_default().to_string();
  for part in parts {
    let mut chars = part.chars();
    out.extend(chars.next().map(|c| c.to_ascii_uppercase()));
    out.push_str(chars.as_str());
  }
  out
}
