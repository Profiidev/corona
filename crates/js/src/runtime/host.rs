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
use serde::{Serialize, de::DeserializeOwned};
use specta::{
  Type, Types,
  datatype::{DataType, Fields, NamedReference, NamedReferenceType, Primitive, Reference},
  function::FunctionArg,
};

use crate::{app, runtime::scope::app_unchecked};

/// The app, injected into a host fn without taking a JS argument. Usable in sync fns and
/// in the polls of async ones, which run in the runtime driver.
pub struct Cx;

impl Deref for Cx {
  type Target = App;
  fn deref(&self) -> &App {
    // SAFETY: lives for the borrow of the `Cx`, which a host fn holds only inside one call
    unsafe { app_unchecked() }
  }
}

impl DerefMut for Cx {
  fn deref_mut(&mut self) -> &mut App {
    // SAFETY: as in `deref`
    unsafe { app_unchecked() }
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
    // SAFETY: as in `Cx::deref`
    unsafe { app_unchecked() }.global::<G>()
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

/// A value crossing as JSON through its serde impls, typed as `T` (serde attributes
/// applied): for host types that derive `Serialize`/`Deserialize` and `Type`.
pub struct Json<T>(pub T);

/// `read` with its value crossing as JSON, for [`ModuleBuilder::watch`](crate::ModuleBuilder::watch)
pub fn json<R>(read: impl Fn(&App) -> R + 'static) -> impl Fn(&App) -> Json<R> + 'static {
  move |cx| Json(read(cx))
}

impl<'js, T: Serialize> IntoJs<'js> for Json<T> {
  fn into_js(self, ctx: &Ctx<'js>) -> Result<Value<'js>> {
    let text =
      serde_json::to_string(&self.0).map_err(|e| Exception::throw_internal(ctx, &e.to_string()))?;
    ctx.json_parse(text)
  }
}

impl<'js, T: DeserializeOwned> FromJs<'js> for Json<T> {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    // `undefined` has no JSON
    let text = match ctx.json_stringify(value)? {
      Some(text) => text.to_string()?,
      None => "null".into(),
    };
    serde_json::from_str(&text)
      .map(Json)
      .map_err(|e| Exception::throw_type(ctx, &e.to_string()))
  }
}

impl<T: Type> Type for Json<T> {
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

/// specta-typescript writes floats as `number | null` (serde turns NaN into null) and refuses
/// 64-bit ints (they would be `bigint`), but rquickjs passes both as numbers, as JSON does
/// for `serde_json::Value`: makes them `i32`, also written `number`. ponytail: generic
/// arguments of references stay as they are, map them too if a generic host type takes floats
pub(crate) fn floats(dt: &mut DataType) {
  match dt {
    DataType::Primitive(
      p @ (Primitive::f16
      | Primitive::f32
      | Primitive::f64
      | Primitive::i64
      | Primitive::u64
      | Primitive::i128
      | Primitive::u128
      | Primitive::isize
      | Primitive::usize),
    ) => *p = Primitive::i32,
    DataType::Reference(Reference::Named(NamedReference {
      inner: NamedReferenceType::Inline { dt, .. },
      ..
    })) => floats(dt),
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
