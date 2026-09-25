use std::{collections::BTreeMap, future::Future, marker::PhantomData, ops::Deref, pin::Pin};

use corona_macros::all_tuples;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use ts_rs::{Config, TS, TypeVisitor};

use gpui_kit::{App, Global};
use gpui_shell::{HostArguments, HostError, HostModule, HostResult, HostValue, with_current_app};

/// Builds a `HostModule` and generates its TypeScript declarations, including every type the
/// functions use, from the Rust signatures.
pub struct Module {
  module: HostModule,
  functions: Vec<String>,
  types: Types,
}

impl Module {
  pub fn new(name: impl Into<String>) -> Self {
    Self {
      module: HostModule::new(name),
      functions: Vec::new(),
      types: Types::default(),
    }
  }

  /// `f` comes from `#[host_fn]` or `named!`, which supply the script name and parameter names.
  pub fn func<I, F: IntoHostFn<I> + 'static>(mut self, f: Named<F>) -> Self {
    let name = f.name;
    let (params, ret) = F::HostFn::signature(&mut self.types);
    debug_assert_eq!(
      f.names.len(),
      params.len(),
      "host function `{name}` names {} params but takes {}",
      f.names.len(),
      params.len()
    );
    let params: Vec<String> = f
      .names
      .iter()
      .zip(params)
      .filter_map(|(arg, ty)| Some(format!("{arg}: {}", ty?)))
      .collect();
    let ret = if F::HostFn::ASYNC {
      format!("Promise<{ret}>")
    } else {
      ret
    };
    self.functions.push(format!(
      "export function {name}({}): {ret};",
      params.join(", ")
    ));

    self.module = register(self.module, name, f.f.into_host_fn());
    self
  }
}

/// Registers `f` as a sync or async function, whichever its return type is.
fn register(module: HostModule, name: impl Into<String>, f: impl HostFn + 'static) -> HostModule {
  if f.is_async() {
    module.async_function(name, move |args| match f.call(args) {
      HostOutput::Pending(future) => Ok(future),
      HostOutput::Ready(result) => result.map(|value| Box::pin(async { Ok(value) }) as HostFuture),
    })
  } else {
    module.function(name, move |args| match f.call(args) {
      HostOutput::Ready(result) => result,
      HostOutput::Pending(_) => unreachable!("a sync host fn returned a future"),
    })
  }
}

/// A host fn with its script name and the names of all its params, context params included.
pub struct Named<F> {
  name: &'static str,
  names: &'static [&'static str],
  f: F,
}

impl<F> Named<F> {
  pub const fn new(name: &'static str, names: &'static [&'static str], f: F) -> Self {
    Self { name, names, f }
  }
}

impl From<Module> for HostModule {
  fn from(module: Module) -> Self {
    let mut declarations: Vec<String> = module.types.decls.into_values().collect();
    declarations.extend(module.functions);
    module.module.declarations(declarations.join("\n"))
  }
}

/// Collects the declarations of every named type reached from the function signatures.
pub struct Types {
  cfg: Config,
  decls: BTreeMap<String, String>,
}

impl Default for Types {
  fn default() -> Self {
    // JS numbers are f64, so `i64`/`u64` are `number` too, not `bigint`.
    Self {
      cfg: Config::new().with_large_int("number"),
      decls: BTreeMap::new(),
    }
  }
}

impl Types {
  /// The TypeScript name of `T`, declaring `T` and everything it depends on.
  fn add<T: TS + 'static + ?Sized>(&mut self) -> String {
    self.visit::<T>();
    T::name(&self.cfg)
  }
}

impl TypeVisitor for Types {
  fn visit<T: TS + 'static + ?Sized>(&mut self) {
    // Only derived types have an output path; primitives and wrappers (`Vec`, `Option`) are inlined.
    if T::output_path().is_some() {
      let ident = T::ident(&self.cfg);
      if self.decls.contains_key(&ident) {
        return;
      }
      let docs = T::docs().unwrap_or_default();
      let decl = format!("{docs}export {}", T::decl(&self.cfg));
      self.decls.insert(ident, decl);
    }
    T::visit_dependencies(self);
    T::visit_generics(self);
  }
}

pub trait HostModuleExt {
  fn func<I>(self, name: impl Into<String>, f: impl IntoHostFn<I> + 'static) -> Self;
}

impl HostModuleExt for HostModule {
  fn func<I>(self, name: impl Into<String>, f: impl IntoHostFn<I> + 'static) -> Self {
    register(self, name, f.into_host_fn())
  }
}

pub trait HostFn {
  /// Whether `call` returns `HostOutput::Pending`; decides how the fn is registered.
  const ASYNC: bool;

  fn call(&self, args: &HostArguments) -> HostOutput;

  fn is_async(&self) -> bool
  where
    Self: Sized,
  {
    Self::ASYNC
  }

  /// The TypeScript type of every param (`None` for context params) and of the return value.
  fn signature(types: &mut Types) -> (Vec<Option<String>>, String)
  where
    Self: Sized;
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

  /// The TypeScript type of the script argument, `None` for context params.
  fn ts_type(types: &mut Types) -> Option<String>;
}

/// Same as gpui-shell's private `HostFuture`, what `HostModule::async_function` boxes to.
pub type HostFuture = Pin<Box<dyn Future<Output = HostResult> + Send>>;

pub enum HostOutput {
  Ready(HostResult),
  Pending(HostFuture),
}

/// `M` only separates the serde impl from the error impl, which would overlap otherwise.
///
/// Sync impls implement `result`, async ones `output`; each defaults to the other.
pub trait HostReturn<M>: Sized {
  const ASYNC: bool = false;

  fn result(self) -> HostResult {
    match self.output() {
      HostOutput::Ready(result) => result,
      HostOutput::Pending(_) => Err(HostError::new("async host fn result read synchronously")),
    }
  }

  fn output(self) -> HostOutput {
    HostOutput::Ready(self.result())
  }

  /// The TypeScript type of the value; `Promise` is added for async fns by `Module`.
  fn ts_type(types: &mut Types) -> String;
}

pub struct Async<M>(PhantomData<M>);

/// An async host fn returns its future: the fn body runs on the main thread with its params, the
/// future on the background executor, so it must be `Send + 'static` and cannot hold `Cx`/`Glob`.
impl<Fut, M: 'static> HostReturn<Async<M>> for Fut
where
  Fut: Future + Send + 'static,
  Fut::Output: HostReturn<M>,
{
  const ASYNC: bool = true;

  fn output(self) -> HostOutput {
    HostOutput::Pending(Box::pin(async move {
      match self.await.output() {
        HostOutput::Ready(result) => result,
        HostOutput::Pending(future) => future.await,
      }
    }))
  }

  fn ts_type(types: &mut Types) -> String {
    Fut::Output::ts_type(types)
  }
}

impl<T: Serialize + TS + 'static> HostReturn<HostValue> for T {
  fn result(self) -> HostResult {
    let value = serde_json::to_value(self).map_err(|e| HostError::new(e.to_string()))?;
    Ok(to_host(value))
  }

  fn ts_type(types: &mut Types) -> String {
    types.add::<T>()
  }
}

// What an `anyhow::Error` becomes in the script.
#[derive(Serialize, TS)]
#[ts(rename = "Error")]
struct ErrorValue {
  message: String,
}

// `Option` and `Result` are only covered for `anyhow::Error`. A generic `Option<T: HostReturn>` impl
// would also match serializable `Option<u32>`, next to the serde impl, and inference fails (E0283).
impl HostReturn<anyhow::Error> for anyhow::Error {
  fn result(self) -> HostResult {
    ErrorValue {
      message: self.to_string(),
    }
    .result()
  }

  fn ts_type(types: &mut Types) -> String {
    types.add::<ErrorValue>()
  }
}

impl HostReturn<Option<anyhow::Error>> for Option<anyhow::Error> {
  fn result(self) -> HostResult {
    self.map_or(Ok(HostValue::Null), anyhow::Error::result)
  }

  fn ts_type(types: &mut Types) -> String {
    format!("{} | null", types.add::<ErrorValue>())
  }
}

impl<T: HostReturn<M>, M> HostReturn<Result<M, anyhow::Error>> for anyhow::Result<T> {
  // `Ok(future)` makes the fn async; `Err` then resolves the promise with the error.
  const ASYNC: bool = T::ASYNC;

  fn output(self) -> HostOutput {
    match self {
      Ok(value) => value.output(),
      Err(error) => error.output(),
    }
  }

  fn ts_type(types: &mut Types) -> String {
    format!("{} | {}", T::ts_type(types), types.add::<ErrorValue>())
  }
}

impl<T: DeserializeOwned + TS + 'static> HostParam for T {
  type Item<'a> = T;

  #[allow(clippy::needless_lifetimes)]
  fn get_param<'a>(args: &HostArguments, pos: &mut usize, _: &'a App) -> Result<T, HostError> {
    let value = deserialize(args, *pos);
    *pos += 1;
    value
  }

  fn ts_type(types: &mut Types) -> Option<String> {
    Some(types.add::<T>())
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

  fn ts_type(_: &mut Types) -> Option<String> {
    None
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
  fn ts_type(_: &mut Types) -> Option<String> {
    None
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
      const ASYNC: bool = R::ASYNC;

      fn call(&self, args: &HostArguments) -> HostOutput {
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
          Ok(call_inner(&self.f, $($params),*).output())
        })
        .unwrap_or_else(|| Err(no_app()))
        .unwrap_or_else(|error| HostOutput::Ready(Err(error)))
      }

      fn signature(types: &mut Types) -> (Vec<Option<String>>, String) {
        let params = vec![$($params::ts_type(types)),*];
        (params, R::ts_type(types))
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
      const ASYNC: bool = R::ASYNC;

      fn call(&self, args: &HostArguments) -> HostOutput {
        with_current_app(|cx| {
          #[allow(unused_mut)]
          let mut pos = 0;
          $(
            let $params = $params::get_param(args, &mut pos, cx)?;
          )*
          Ok((self.f)(cx, $($params),*).output())
        })
        .unwrap_or_else(|| Err(no_app()))
        .unwrap_or_else(|error| HostOutput::Ready(Err(error)))
      }

      fn signature(types: &mut Types) -> (Vec<Option<String>>, String) {
        // `None` for the `&mut App`.
        let params = vec![None, $($params::ts_type(types)),*];
        (params, R::ts_type(types))
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

all_tuples!(impl_host_fn, 0, 16, F);

#[cfg(test)]
mod tests {
  use corona_macros::{host_fn, named};
  use gpui_shell::HostObject;
  use serde::Deserialize;

  use crate::integration::pipewire::Pipewire;

  use super::*;

  #[derive(Serialize, Deserialize, TS, Debug, PartialEq)]
  #[serde(rename_all = "snake_case")]
  enum Kind {
    Sink,
    Source,
  }

  /// An audio node.
  #[derive(Serialize, Deserialize, TS)]
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
      anyhow::Result::<u32>::Err(anyhow::anyhow!("boom"))
        .result()
        .unwrap(),
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
      .func("nested", || -> anyhow::Result<Option<anyhow::Error>> {
        Ok(None)
      });
    assert_eq!(module.function_names().len(), 11);
  }

  #[host_fn]
  fn get_node(_pw: Glob<Pipewire>, id: u32) -> Option<Node> {
    let _ = id;
    None
  }

  #[host_fn]
  async fn fetch_nodes(count: usize) -> anyhow::Result<Vec<Node>> {
    let _ = count;
    Ok(Vec::new())
  }

  /// Polls a future that never waits, which is all the async test fns do.
  fn poll_ready(mut future: HostFuture) -> HostResult {
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    match future.as_mut().poll(&mut cx) {
      std::task::Poll::Ready(result) => result,
      std::task::Poll::Pending => panic!("test future is pending"),
    }
  }

  #[test]
  fn async_output() {
    let HostOutput::Pending(future) = async { 3 }.output() else {
      panic!("an async block is async");
    };
    assert_eq!(poll_ready(future).unwrap(), HostValue::Number(3.0));

    // `Err` before the future exists still resolves to the error value.
    let result: anyhow::Result<std::future::Ready<u32>> = Err(anyhow::anyhow!("boom"));
    let HostOutput::Ready(value) = result.output() else {
      panic!("an `Err` has no future");
    };
    assert_eq!(
      value.unwrap(),
      HostObject::new().field("message", "boom").into()
    );
  }

  #[test]
  fn declarations() {
    let module: HostModule = Module::new("test")
      .func(get_node)
      .func(fetch_nodes)
      .func(named!("double", async |id: u32| id * 2))
      .func(named!("delayed", |pw: Glob<Pipewire>, id: u32| {
        let _ = pw;
        async move { anyhow::Ok(id) }
      }))
      .func(named!("set", |nodes: Vec<Node>, _force: bool| {
        let _ = nodes;
        None::<anyhow::Error>
      }))
      .func(named!("count", |_cx: &mut App,
                             (a, b): (u32, u32)|
       -> anyhow::Result<u64> {
        Ok((a + b).into())
      }))
      .into();

    module.validate().unwrap();
    assert_eq!(
      module.declared().unwrap(),
      [
        "export type Error = { message: string, };",
        r#"export type Kind = "sink" | "source";"#,
        "/**\n * An audio node.\n */",
        r#"export type Node = { id: number, kind: Kind, name: string | null, };"#,
        "export function getNode(id: number): Node | null;",
        "export function fetchNodes(count: number): Promise<Array<Node> | Error>;",
        "export function double(id: number): Promise<number>;",
        "export function delayed(id: number): Promise<number | Error>;",
        "export function set(nodes: Array<Node>, force: boolean): Error | null;",
        "export function count(arg1: [number, number]): number | Error;",
      ]
      .join("\n")
    );
  }
}
