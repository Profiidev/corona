//! Plugin runtime on the official `rquickjs` (see `docs/js-runtime.md`).

use std::{
  future::Future,
  pin::pin,
  task::{Context, Poll, Waker},
};

use rquickjs::{AsyncContext, Ctx};

/// Where the host shows a tooltip itself, as gpui draws its own inside the window, which a bar
/// clips. It gives the view that lives as long as the hover, or none to leave it to gpui.
pub struct TooltipHost(
  pub fn(&str, &mut gpui_kit::Window, &mut gpui_kit::App) -> Option<gpui_kit::AnyView>,
);

impl gpui_kit::Global for TooltipHost {}

pub use element::{
  Child, El, Frame, Id, Named, Node, On, Payload, TsType,
  components::constructor,
  define, frame_or_throw, jsdoc, map, miss, new_el, restyle,
  style::{Abs, Color, Def, Fill, Len, LineHeight, Radius},
  with_node,
};
pub use runtime::{
  caps::{Access, Capabilities, CapabilityError, ExecuteGrant, Grant, HttpRequestGrant},
  handler::{Handler, Spec, encode},
  host::{Cx, Glob, Json, Opt, Throws, json},
  limits::{Trip, TripHook, take_tripped},
  module::{Data, ModuleBuilder, d_ts},
  plugin::Plugin,
  registry::{PluginRef, Registered, ScriptExt, ScriptModule, ScriptRegistry},
  reload::{POLL, reload_view, watch},
  scope::{Enter, app, frame, window},
  service::Service,
  watch::{Callback, Listener, Unsubscribe, Watches, track},
};
pub use stdlib::D_TS as STDLIB_D_TS;
pub use view::{
  FocusHandle, ScriptView, ViewCx, ViewHandle,
  entity::{Retained, ScrollHandle},
  kit::Kit,
  window::ScriptRoot,
};

#[doc(hidden)]
pub use {gpui_kit, rquickjs, specta};

mod element;
mod runtime;
mod stdlib;
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
