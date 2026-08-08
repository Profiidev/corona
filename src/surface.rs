//! Shared layer-surface creation helper — the two-phase "create surface, wait for the
//! compositor's first configure, then build the EGL window" dance is identical for the bar,
//! notifications, the OSD, and the calendar; only the anchor/size/layer and which UI ends up on
//! it differ.

use anyhow::{Context, Result};
use smithay_client_toolkit::compositor::CompositorState;
use smithay_client_toolkit::shell::WaylandSurface as _;
use smithay_client_toolkit::shell::wlr_layer::{
  Anchor, KeyboardInteractivity, Layer, LayerShell, LayerSurface,
};
use wayland_client::protocol::wl_output;
use wayland_client::{Proxy as _, QueueHandle};

use crate::app::AppState;
use crate::egl::RenderContextFactory;
use crate::platform::CoronaPlatform;
use crate::window::CoronaWindow;

pub struct SurfaceSpec<'a> {
  pub namespace: &'static str,
  pub layer: Layer,
  pub anchor: Anchor,
  /// 0 means "compositor picks" (used for the bar's width, anchored to both left+right edges).
  pub width: u32,
  pub height: u32,
  pub exclusive_zone: i32,
  pub margin: (i32, i32, i32, i32),
  /// Which output to place this surface on. `None` lets the compositor pick (fine for
  /// transient popups); the bar passes a specific output so it shows up on every monitor.
  pub output: Option<&'a wl_output::WlOutput>,
}

pub fn spawn_layer_surface(
  compositor: &CompositorState,
  layer_shell: &LayerShell,
  qh: &QueueHandle<AppState>,
  spec: &SurfaceSpec,
) -> LayerSurface {
  let wl_surface = compositor.create_surface(qh);
  let layer_surface = layer_shell.create_layer_surface(
    qh,
    wl_surface,
    spec.layer,
    Some(spec.namespace),
    spec.output,
  );
  layer_surface.set_anchor(spec.anchor);
  layer_surface.set_size(spec.width, spec.height);
  layer_surface.set_exclusive_zone(spec.exclusive_zone);
  layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);
  let (t, r, b, l) = spec.margin;
  layer_surface.set_margin(t, r, b, l);
  layer_surface.wl_surface().commit();
  layer_surface
}

/// Builds the EGL context + `CoronaWindow` for a surface once its real size is known (first
/// `configure`), and registers it with the platform so the next component `::new()` picks it up.
pub fn build_window(
  render_factory: &RenderContextFactory,
  layer_surface: &LayerSurface,
  platform: &CoronaPlatform,
  width: u32,
  height: u32,
) -> Result<std::rc::Rc<CoronaWindow>> {
  let context = render_factory
    .create_context(&layer_surface.wl_surface().id(), width, height)
    .context("failed to create EGL context")?;
  let window = CoronaWindow::new(context, slint::PhysicalSize::new(width, height))
    .map_err(|e| anyhow::anyhow!("failed to create Slint window adapter: {e}"))?;
  platform.add_window(std::rc::Rc::clone(&window));
  Ok(window)
}
