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

pub use components::constructor;
pub use el::{
  Child, El, Frame, Id, Named, Node, On, Payload, TsType, frame_or_throw, jsdoc, map, new_el,
  restyle, with_node,
};
pub use entity::{Retained, ScrollHandle};
pub use host::{Cx, Glob, Opt, Throws};
pub use kit::GpuiKit;
pub use module::{ModuleBuilder, d_ts};
pub use registry::{PluginRef, Registered, ScriptExt, ScriptModule, ScriptRegistry};
pub use scope::{Enter, app, frame, window};
pub use style::{Abs, Color, Def, Fill, Len, LineHeight, Radius};
pub use view::{FocusHandle, ScriptView, ViewCx, ViewHandle};
pub use watch::{Callback, Watches, track};
pub use window::ScriptRoot;

#[doc(hidden)]
pub use {gpui_kit, rquickjs, specta};

mod background;
mod components;
mod el;
mod entity;
mod events;
mod host;
mod kit;
mod module;
mod registry;
mod scope;
mod style;
mod theme;
mod view;
mod watch;
mod window;

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
