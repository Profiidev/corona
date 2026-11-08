//! Shared plumbing of the stdlib: the IO runtime, promises settled from it, errors and bytes.

use std::{
  future::Future,
  pin::Pin,
  sync::LazyLock,
  task::{Context, Poll},
};

use rquickjs::{
  ArrayBuffer, Constructor, Ctx, Exception, IntoJs, Promise, Result, TypedArray, Value,
};
use tokio::{runtime::Runtime, task::JoinHandle};

use crate::runtime::caps::CapabilityError;

/// Runs the IO of every plugin; JS awaits its tasks from the gpui executor.
static RT: LazyLock<Runtime> = LazyLock::new(|| {
  let _ = rustls::crypto::ring::default_provider().install_default();
  tokio::runtime::Builder::new_multi_thread()
    .worker_threads(2)
    .thread_name("corona-js-io")
    .enable_all()
    .build()
    .expect("js io runtime")
});

/// A task on the IO runtime, aborted when dropped: when its runtime goes away, its sockets
/// and streams close.
pub(crate) struct Job<T>(JoinHandle<T>);

impl<T> Future for Job<T> {
  type Output = std::result::Result<T, Fail>;
  fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    Pin::new(&mut self.0)
      .poll(cx)
      .map(|r| r.map_err(|e| Fail::Error(format!("io task failed: {e}"))))
  }
}

impl<T> Drop for Job<T> {
  fn drop(&mut self) {
    self.0.abort();
  }
}

/// Runs `fut` on the IO runtime.
pub(crate) fn io<T: Send + 'static>(
  fut: impl Future<Output = std::result::Result<T, Fail>> + Send + 'static,
) -> impl Future<Output = std::result::Result<T, Fail>> {
  let job = Job(RT.spawn(fut));
  async move { job.await? }
}

/// Runs `f` on a blocking thread of the IO runtime.
pub(crate) fn blocking<T: Send + 'static>(
  f: impl FnOnce() -> std::result::Result<T, Fail> + Send + 'static,
) -> impl Future<Output = std::result::Result<T, Fail>> {
  let job = Job(RT.spawn_blocking(f));
  async move { job.await? }
}

/// Makes the IO runtime current, for APIs that register with it when called
pub(crate) fn enter() -> tokio::runtime::EnterGuard<'static> {
  RT.enter()
}

/// Spawns on the IO runtime, detached.
pub(crate) fn detach(fut: impl Future<Output = ()> + Send + 'static) {
  RT.spawn(fut);
}

/// What a stdlib call throws or rejects with: the JS error class and message
#[derive(Debug)]
pub(crate) enum Fail {
  Type(String),
  Range(String),
  Error(String),
}

impl Fail {
  /// Throws it from a sync fn
  pub(crate) fn throw(self, ctx: &Ctx<'_>) -> rquickjs::Error {
    match self {
      Fail::Type(m) => Exception::throw_type(ctx, &m),
      Fail::Range(m) => Exception::throw_range(ctx, &m),
      Fail::Error(m) => Exception::throw_message(ctx, &m),
    }
  }

  fn into_value<'js>(self, ctx: &Ctx<'js>) -> Result<Value<'js>> {
    let (class, message) = match self {
      Fail::Type(m) => ("TypeError", m),
      Fail::Range(m) => ("RangeError", m),
      Fail::Error(m) => ("Error", m),
    };
    let class: Constructor = ctx.globals().get(class)?;
    class.construct((message,))
  }
}

impl From<CapabilityError> for Fail {
  fn from(e: CapabilityError) -> Self {
    Fail::Type(e.to_string())
  }
}

impl From<String> for Fail {
  fn from(e: String) -> Self {
    Fail::Error(e)
  }
}

pub(crate) type Res<T> = std::result::Result<T, Fail>;

/// A promise settled by `fut`, which runs in the runtime's driver
pub(crate) fn promise<'js, T, F>(ctx: &Ctx<'js>, fut: F) -> Result<Promise<'js>>
where
  T: IntoJs<'js> + 'js,
  F: Future<Output = Res<T>> + 'js,
{
  let (promise, resolve, reject) = ctx.promise()?;
  let ctx = ctx.clone();
  ctx.clone().spawn(async move {
    let out = fut.await;
    let settled = match out {
      Ok(value) => value
        .into_js(&ctx)
        .and_then(|v| resolve.call::<_, ()>((v,))),
      Err(fail) => fail
        .into_value(&ctx)
        .and_then(|v| reject.call::<_, ()>((v,))),
    };
    if let Err(e) = settled {
      // a conversion that threw: reject with what it threw
      let thrown = ctx.catch();
      let _ = reject.call::<_, ()>((thrown,));
      tracing::debug!("settling a promise failed: {e}");
    }
  });
  Ok(promise)
}

/// Bytes handed to JS, as a `Uint8Array`
pub(crate) struct Bytes(pub Vec<u8>);

impl<'js> IntoJs<'js> for Bytes {
  fn into_js(self, ctx: &Ctx<'js>) -> Result<Value<'js>> {
    TypedArray::<u8>::new(ctx.clone(), self.0).map(|a| a.into_value())
  }
}

/// `null` for `None`, where rquickjs would hand out `undefined`
pub struct Null<T>(pub Option<T>);

impl<'js, T: IntoJs<'js>> IntoJs<'js> for Null<T> {
  fn into_js(self, ctx: &Ctx<'js>) -> Result<Value<'js>> {
    match self.0 {
      Some(v) => v.into_js(ctx),
      None => Ok(Value::new_null(ctx.clone())),
    }
  }
}

/// Bytes handed to JS, as a `Buffer`
pub(crate) struct Buf(pub Vec<u8>);

impl<'js> IntoJs<'js> for Buf {
  fn into_js(self, ctx: &Ctx<'js>) -> Result<Value<'js>> {
    let buffer: Constructor = ctx.globals().get("Buffer")?;
    buffer.construct((ArrayBuffer::new(ctx.clone(), self.0)?,))
  }
}

/// The bytes of a string (UTF-8), `Uint8Array` or `ArrayBuffer`
pub(crate) fn bytes(value: &Value<'_>, what: &str) -> Result<Vec<u8>> {
  if let Some(s) = value.as_string() {
    return Ok(s.to_string()?.into_bytes());
  }
  if let Ok(array) = TypedArray::<u8>::from_value(value.clone())
    // SAFETY: copied right away, nothing runs JS meanwhile
    && let Some(bytes) = unsafe { array.as_bytes() }
  {
    return Ok(bytes.to_vec());
  }
  if let Some(buffer) = ArrayBuffer::from_value(value.clone())
    // SAFETY: copied right away, nothing runs JS meanwhile
    && let Some(bytes) = unsafe { buffer.as_bytes() }
  {
    return Ok(bytes.to_vec());
  }
  Err(Exception::throw_type(
    value.ctx(),
    &format!("{what} expects a string or Uint8Array"),
  ))
}
