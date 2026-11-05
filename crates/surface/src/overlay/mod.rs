//! Fullscreen layer-shell overlays: their window options, the state of the
//! open ones and the shared password dialog

use anyhow::Context as _;
use corona_compositor::CompositorExt;
use corona_config::APP_NAME;
use corona_utils::display::display_id_for;
use gpui_kit::{
  AnyWindowHandle, App, AppContext, Bounds, DisplayId, Entity, Global, Pixels, Render, Size,
  Styled, Window, WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowKind,
  WindowOptions,
  base::Root,
  layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
  point, px,
};

pub mod password;
pub mod wallpaper;

pub trait OverlayState: Global + Sized {
  fn overlays(&self) -> &[AnyWindowHandle];

  fn get(cx: &mut App) -> Option<&mut Self> {
    if cx.has_global::<Self>() {
      Some(cx.global_mut::<Self>())
    } else {
      None
    }
  }

  fn close(current: Option<&mut Window>, cx: &mut App) {
    if !cx.has_global::<Self>() {
      return;
    }
    let state = cx.remove_global::<Self>();

    for handle in state.overlays() {
      let _ = handle.update(cx, |_, window, _| window.remove_window());
    }
    if let Some(window) = current {
      window.remove_window();
    }
  }

  fn refresh_all(cx: &mut App) {
    let Some(state) = Self::get(cx) else {
      return;
    };
    for handle in state.overlays().to_vec() {
      let _ = handle.update(cx, |_, window, _| window.refresh());
    }
  }
}

/// A transparent layer-shell window `size` big; a zero size follows the anchors
pub fn layer_options(
  layer_shell: LayerShellOptions,
  display: Option<DisplayId>,
  size: Size<Pixels>,
) -> WindowOptions {
  WindowOptions {
    kind: WindowKind::LayerShell(layer_shell),
    window_background: WindowBackgroundAppearance::Transparent,
    window_decorations: Some(WindowDecorations::Client),
    display_id: display,
    app_id: Some(APP_NAME.to_string()),
    titlebar: None,
    window_bounds: Some(WindowBounds::Windowed(Bounds {
      origin: point(px(0.), px(0.)),
      size,
    })),
    ..Default::default()
  }
}

/// The root of a window showing `view`, transparent around it
pub fn root(view: Entity<impl Render>, window: &mut Window, cx: &mut App) -> Entity<Root> {
  cx.new(|cx| Root::new(view, window, cx).bg(gpui_kit::transparent_black()))
}

/// Every edge anchored, over everything and taking the keyboard
fn fullscreen_layer(namespace: &str) -> LayerShellOptions {
  LayerShellOptions {
    anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT | Anchor::BOTTOM,
    exclusive_zone: Some(px(-1.)),
    exclusive_edge: None,
    margin: None,
    layer: Layer::Overlay,
    namespace: namespace.to_string(),
    keyboard_interactivity: KeyboardInteractivity::Exclusive,
  }
}

pub fn fullscreen_options(namespace: &str, display: DisplayId) -> WindowOptions {
  WindowOptions {
    window_background: WindowBackgroundAppearance::Opaque,
    ..layer_options(fullscreen_layer(namespace), Some(display), Size::default())
  }
}

/// A transparent fullscreen overlay on the focused monitor, or the primary display, showing the
/// view `build` makes; `tweak` adjusts its layer-shell options
pub fn open_on_focused<V: Render>(
  namespace: &str,
  tweak: impl FnOnce(&mut LayerShellOptions),
  cx: &mut App,
  build: impl FnOnce(&mut Window, &mut App) -> Entity<V>,
) -> anyhow::Result<AnyWindowHandle> {
  let monitor = cx.compositor().active_monitor(cx).name.clone();
  let display = display_id_for(&monitor, cx)
    .or_else(|| cx.primary_display().map(|d| d.id()))
    .context("no display for the focused monitor")?;
  let mut layer = fullscreen_layer(namespace);
  tweak(&mut layer);
  let handle = cx.open_window(
    layer_options(layer, Some(display), Size::default()),
    |window, cx| {
      let view = build(window, cx);
      root(view, window, cx)
    },
  )?;
  Ok(handle.into())
}
