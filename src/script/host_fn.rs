use std::{marker::PhantomData, ops::Deref};

use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

use gpui_kit::{App, Global};
use gpui_shell::{
  HostArguments, HostError, HostModule, HostObject, HostResult, HostValue, with_current_app,
};

pub trait HostModuleExt {
  fn func<I>(self, name: impl Into<String>, f: impl IntoHostFn<I> + 'static) -> Self;
}

impl HostModuleExt for HostModule {
  fn func<I>(self, name: impl Into<String>, f: impl IntoHostFn<I> + 'static) -> Self {
    let f = f.into_host_fn();
    self.function(name, move |args| f.call(args))
  }
}

pub trait HostFn {
  fn call(&self, args: &HostArguments) -> HostResult;
}

pub struct FunctionHostFn<Input, F> {
  f: F,
  marker: PhantomData<fn() -> Input>,
}

pub trait IntoHostFn<Input> {
  type HostFn: HostFn + 'static;

  fn into_host_fn(self) -> Self::HostFn;
}

/// Marks functions taking `&mut App` as first argument, so their `IntoHostFn` impl does not
/// overlap with the plain one.
pub struct WithApp<Input>(PhantomData<Input>);

pub trait HostParam {
  type Item<'a>;

  /// Reads the param; script arguments advance `pos`, context params (`Cx`, globals) do not.
  fn get_param<'a>(
    args: &HostArguments,
    pos: &mut usize,
    cx: &'a App,
  ) -> Result<Self::Item<'a>, HostError>;
}

/// `M` only separates the serde impl from the error impl, which would overlap otherwise.
pub trait HostReturn<M> {
  fn result(self) -> HostResult;
}

impl<T: Serialize> HostReturn<HostValue> for T {
  fn result(self) -> HostResult {
    let value = serde_json::to_value(self).map_err(|e| HostError::new(e.to_string()))?;
    Ok(to_host(value))
  }
}

// `Option` and `Result` are only covered for `anyhow::Error`. A generic `Option<T: HostReturn>` impl
// would also match serializable `Option<u32>`, next to the serde impl, and inference fails (E0283).
impl HostReturn<anyhow::Error> for anyhow::Error {
  fn result(self) -> HostResult {
    Ok(HostObject::new().field("message", self.to_string()).into())
  }
}

impl HostReturn<Option<anyhow::Error>> for Option<anyhow::Error> {
  fn result(self) -> HostResult {
    self.map_or(Ok(HostValue::Null), anyhow::Error::result)
  }
}

impl<T: HostReturn<M>, M> HostReturn<Result<M, anyhow::Error>> for anyhow::Result<T> {
  fn result(self) -> HostResult {
    match self {
      Ok(value) => value.result(),
      Err(error) => error.result(),
    }
  }
}

impl<T: DeserializeOwned + 'static> HostParam for T {
  type Item<'a> = T;

  #[allow(clippy::needless_lifetimes)]
  fn get_param<'a>(args: &HostArguments, pos: &mut usize, _: &'a App) -> Result<T, HostError> {
    let value = deserialize(args, *pos);
    *pos += 1;
    value
  }
}

/// Shared access to the app. A plain `&App` param would overlap with the serde impl.
pub struct Cx<'a>(&'a App);

impl Deref for Cx<'_> {
  type Target = App;

  fn deref(&self) -> &App {
    self.0
  }
}

impl HostParam for Cx<'_> {
  type Item<'a> = Cx<'a>;

  fn get_param<'a>(_: &HostArguments, _: &mut usize, cx: &'a App) -> Result<Cx<'a>, HostError> {
    Ok(Cx(cx))
  }
}

/// Read-only access to a gpui global. A plain `&G` param would overlap with the serde impl.
pub struct Glob<'a, G>(&'a G);

impl<G> Deref for Glob<'_, G> {
  type Target = G;

  fn deref(&self) -> &G {
    self.0
  }
}

impl<G: Global> HostParam for Glob<'_, G> {
  type Item<'a> = Glob<'a, G>;

  fn get_param<'a>(
    _: &HostArguments,
    _: &mut usize,
    cx: &'a App,
  ) -> Result<Glob<'a, G>, HostError> {
    cx.try_global::<G>()
      .map(Glob)
      .ok_or_else(|| HostError::new(format!("global {} not set", std::any::type_name::<G>())))
  }
}

fn deserialize<T: DeserializeOwned>(args: &HostArguments, pos: usize) -> Result<T, HostError> {
  let value = args.get(pos).map_or(Value::Null, from_host);
  serde_json::from_value(value).map_err(|e| HostError::new(format!("argument {pos}: {e}")))
}

fn no_app() -> HostError {
  HostError::new("host function called outside of an app scope")
}

fn to_host(value: Value) -> HostValue {
  match value {
    Value::Null => HostValue::Null,
    Value::Bool(b) => HostValue::Bool(b),
    Value::Number(n) => HostValue::Number(n.as_f64().unwrap_or(f64::NAN)),
    Value::String(s) => HostValue::Str(s),
    Value::Array(a) => HostValue::Array(a.into_iter().map(to_host).collect()),
    Value::Object(o) => HostValue::Object(o.into_iter().map(|(k, v)| (k, to_host(v))).collect()),
  }
}

fn from_host(value: &HostValue) -> Value {
  match value {
    HostValue::Null => Value::Null,
    HostValue::Bool(b) => Value::Bool(*b),
    HostValue::Number(n) => number(*n),
    HostValue::Str(s) => Value::String(s.clone()),
    HostValue::Array(a) => Value::Array(a.iter().map(from_host).collect()),
    HostValue::Object(o) => {
      Value::Object(o.iter().map(|(k, v)| (k.clone(), from_host(v))).collect())
    }
  }
}

// JS has only f64; integral numbers must become ints or integer params fail to deserialize.
fn number(n: f64) -> Value {
  let rounded = n.round();
  if (n - rounded).abs() >= f64::EPSILON {
    return Value::from(n);
  }
  // `MAX as f64` rounds up to 2^63 / 2^64, so the upper bounds are exclusive.
  if rounded >= i64::MIN as f64 && rounded < i64::MAX as f64 {
    Value::from(rounded as i64)
  } else if rounded >= 0.0 && rounded < u64::MAX as f64 {
    Value::from(rounded as u64)
  } else {
    Value::from(n)
  }
}

macro_rules! impl_host_fn {
  ($($params:ident),*) => {
    #[allow(unused_variables)]
    #[allow(non_snake_case)]
    impl<F, R: HostReturn<M>, M, $($params : HostParam),*> HostFn for FunctionHostFn<(M, ($($params ,)*)), F>
    where
      for<'a, 'b> &'a F: Fn($($params),*) -> R + Fn($(<$params as HostParam>::Item<'b>),*) -> R,
    {
      fn call(&self, args: &HostArguments) -> HostResult {
        #[allow(clippy::too_many_arguments)]
        fn call_inner<R, $($params),*>(f: impl Fn($($params),*) -> R, $($params: $params),*) -> R {
          f($($params),*)
        }

        with_current_app(|cx| {
          #[allow(unused_mut)]
          let mut pos = 0;
          $(
            let $params = $params::get_param(args, &mut pos, cx)?;
          )*
          call_inner(&self.f, $($params),*).result()
        })
        .unwrap_or_else(|| Err(no_app()))
      }
    }

    #[allow(unused_variables)]
    #[allow(non_snake_case)]
    impl<F: 'static, R: HostReturn<M>, M: 'static, $($params : HostParam + 'static),*> IntoHostFn<(M, ($($params ,)*))> for F
    where
      for<'a, 'b> &'a F: Fn($($params),*) -> R + Fn($(<$params as HostParam>::Item<'b>),*) -> R,
    {
      type HostFn = FunctionHostFn<(M, ($($params ,)*)), Self>;

      fn into_host_fn(self) -> Self::HostFn {
        FunctionHostFn { f: self, marker: PhantomData }
      }
    }

    // `&mut App` first: other params must be owned, since the app is borrowed mutably for the call.
    #[allow(unused_variables)]
    #[allow(non_snake_case)]
    impl<F: Fn(&mut App, $($params),*) -> R, R: HostReturn<M>, M, $($params : for<'a> HostParam<Item<'a> = $params>),*> HostFn
      for FunctionHostFn<WithApp<(M, ($($params ,)*))>, F>
    {
      fn call(&self, args: &HostArguments) -> HostResult {
        with_current_app(|cx| {
          #[allow(unused_mut)]
          let mut pos = 0;
          $(
            let $params = $params::get_param(args, &mut pos, cx)?;
          )*
          (self.f)(cx, $($params),*).result()
        })
        .unwrap_or_else(|| Err(no_app()))
      }
    }

    #[allow(unused_variables)]
    #[allow(non_snake_case)]
    impl<F: Fn(&mut App, $($params),*) -> R + 'static, R: HostReturn<M>, M: 'static, $($params : for<'a> HostParam<Item<'a> = $params> + 'static),*>
      IntoHostFn<WithApp<(M, ($($params ,)*))>> for F
    {
      type HostFn = FunctionHostFn<WithApp<(M, ($($params ,)*))>, Self>;

      fn into_host_fn(self) -> Self::HostFn {
        FunctionHostFn { f: self, marker: PhantomData }
      }
    }
  };
}

macro_rules! impl_host_fn_tuples {
  () => {
    impl_host_fn!();
  };
  ($first:ident $(, $rest:ident)*) => {
    impl_host_fn!($first $(, $rest)*);
    impl_host_fn_tuples!($($rest),*);
  };
}

impl_host_fn_tuples!(
  A1, A2, A3, A4, A5, A6, A7, A8, A9, A10, A11, A12, A13, A14, A15, A16, A17, A18, A19, A20
);

#[cfg(test)]
mod tests {
  use gpui_shell::HostObject;
  use serde::Deserialize;

  use super::*;

  #[derive(Serialize, Deserialize, Debug, PartialEq)]
  #[serde(rename_all = "snake_case")]
  enum Kind {
    Sink,
    Source,
  }

  #[derive(Deserialize)]
  struct Node {
    id: u32,
    kind: Kind,
    name: Option<String>,
  }

  #[test]
  fn serde_conversion() {
    let node = HostObject::new().field("id", 3).field("kind", "sink");
    let args = HostArguments::new([node.into(), HostValue::Number(2.5), HostValue::Number(1e19)]);

    let node = deserialize::<Node>(&args, 0).unwrap();
    assert_eq!((node.id, node.kind, node.name), (3, Kind::Sink, None));
    assert_eq!(deserialize::<Option<f64>>(&args, 1).unwrap(), Some(2.5));
    assert_eq!(deserialize::<Option<f64>>(&args, 9).unwrap(), None);
    assert!(deserialize::<u32>(&args, 1).is_err());
    assert_eq!(
      deserialize::<u64>(&args, 2).unwrap(),
      10_000_000_000_000_000_000
    );
    assert!(deserialize::<i64>(&args, 2).is_err());

    assert_eq!(().result().unwrap(), HostValue::Null);
    assert_eq!(None::<anyhow::Error>.result().unwrap(), HostValue::Null);
    assert_eq!(anyhow::Ok(3).result().unwrap(), HostValue::Number(3.0));
    assert_eq!(
      anyhow::Result::<u32>::Err(anyhow::anyhow!("boom")).result().unwrap(),
      HostObject::new().field("message", "boom").into()
    );
    assert_eq!(
      Some(anyhow::anyhow!("boom")).result().unwrap(),
      HostObject::new().field("message", "boom").into()
    );
    assert_eq!(
      Kind::Source.result().unwrap(),
      HostValue::Str("source".into())
    );
  }

  #[test]
  fn param_kinds() {
    use crate::integration::pipewire::Pipewire;

    let module = HostModule::new("test")
      .func("plain", |a: i64, b: String| a + b.len() as i64)
      .func("unit", || ())
      .func("global", |id: u32, _pw: Glob<Pipewire>| id)
      .func(
        "cx",
        |_cx: Cx, _pw: Glob<Pipewire>, name: Option<String>| name,
      )
      .func("mut_app", |_cx: &mut App, id: u32| id * 2)
      .func("mut_app_only", |_cx: &mut App| ())
      .func("error", |fail: bool| {
        fail.then(|| anyhow::anyhow!("failed"))
      })
      .func("result", |fail: bool| -> anyhow::Result<u32> {
        if fail {
          anyhow::bail!("failed")
        }
        Ok(1)
      })
      .func("serde_option", |id: Option<u32>| id)
      .func("serde_result", |id: u32| -> Result<u32, String> { Ok(id) })
      .func("nested", || -> anyhow::Result<Option<anyhow::Error>> { Ok(None) });
    assert_eq!(module.function_names().len(), 11);
  }
}
