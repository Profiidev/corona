//! Plugin runtime on the official `rquickjs` (spike, see `docs/js-runtime.md`).
//!
//! A separate crate because QuickJS cannot be linked twice: the patched rquickjs of
//! `gpui-shell` exports the same C symbols, so no binary can hold both until it is gone.

use std::{
  future::Future,
  pin::pin,
  task::{Context, Poll, Waker},
};

use rquickjs::{AsyncContext, Ctx};

pub use el::{El, Frame, el_d_ts};
pub use host::{Cx, Throws, render_module};
pub use scope::{Enter, app, frame, window};
pub use view::{GpuiKit, ScriptView, ViewCx};

#[doc(hidden)]
pub use {rquickjs, specta};

mod el;
mod host;
mod scope;
mod view;

#[cfg(test)]
mod tests;

/// Polls `fut` once; for runtime calls that only wait on the runtime lock, which is free
/// whenever no JS runs on this thread.
fn now<F: Future>(fut: F) -> F::Output {
  match pin!(fut).poll(&mut Context::from_waker(Waker::noop())) {
    Poll::Ready(out) => out,
    Poll::Pending => panic!("js runtime is busy"),
  }
}

/// Runs `f` in the context, then the promise jobs it queued.
pub fn with<R>(ctx: &AsyncContext, f: impl for<'js> FnOnce(Ctx<'js>) -> R) -> R {
  now(ctx.with(|ctx| {
    let out = f(ctx.clone());
    while ctx.execute_pending_job() {}
    out
  }))
}
