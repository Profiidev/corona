use corona_ipc::IpcServer;
use gpui_kit::{
  AnyWindowHandle, App, Bounds, DisplayId, Global, Size, Window, WindowBackgroundAppearance,
  WindowBounds, WindowDecorations, WindowKind, WindowOptions,
  layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
  point, px,
};

pub mod colorpicker;
pub mod notification;
pub mod screenshot;

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

pub fn register_commands(server: &mut IpcServer) {
  colorpicker::commands::register_commands(server);
  screenshot::commands::register_commands(server);
}

fn fullscreen_options(namespace: &str, display: DisplayId) -> WindowOptions {
  WindowOptions {
    kind: WindowKind::LayerShell(LayerShellOptions {
      anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT | Anchor::BOTTOM,
      exclusive_zone: Some(px(-1.)),
      exclusive_edge: None,
      margin: None,
      layer: Layer::Overlay,
      namespace: namespace.to_string(),
      keyboard_interactivity: KeyboardInteractivity::Exclusive,
    }),
    window_background: WindowBackgroundAppearance::Opaque,
    window_decorations: Some(WindowDecorations::Client),
    display_id: Some(display),
    titlebar: None,
    window_bounds: Some(WindowBounds::Windowed(Bounds {
      origin: point(px(0.), px(0.)),
      size: Size::new(px(0.), px(0.)),
    })),
    ..Default::default()
  }
}
