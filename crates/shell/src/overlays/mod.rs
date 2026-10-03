use corona_ipc::IpcServer;
use gpui_kit::{
  Bounds, DisplayId, Size, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
  layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
  point, px,
};

pub mod colorpicker;
pub mod screenshot;

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
    display_id: Some(display),
    titlebar: None,
    window_bounds: Some(WindowBounds::Windowed(Bounds {
      origin: point(px(0.), px(0.)),
      size: Size::new(px(0.), px(0.)),
    })),
    ..Default::default()
  }
}
