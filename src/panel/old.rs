use std::time::{Duration, Instant};

use gpui_kit::{
  App, AppContext, Bounds, MouseButton, Path, PathBuilder, Pixels, Render, Size, Window,
  WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, canvas,
  component::{ActiveTheme, button::Button, *},
  div,
  layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
  point,
  prelude::*,
  px,
};


impl Panel {
  pub fn create(cx: &mut App) -> Result<gpui_kit::WindowHandle<Root>, anyhow::Error> {
    cx.open_window(
      WindowOptions {
        kind: WindowKind::LayerShell(LayerShellOptions {
          // Covers the whole output — including under the bar, which is what
          // an exclusive zone of -1 asks for — so a click anywhere off the
          // panel reaches this window and dismisses it.
          anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT | Anchor::BOTTOM,
          exclusive_zone: None,
          exclusive_edge: None,
          margin: None,
          layer: Layer::Overlay,
          namespace: "corona_panel".to_string(),
          keyboard_interactivity: KeyboardInteractivity::OnDemand,
        }),
        window_background: WindowBackgroundAppearance::Transparent,
        inactive_frame_interval: None,
        app_id: Some("corona_bar".to_string()),
        titlebar: None,
        // A zero size with opposite anchors means "fill"; a real size would be
        // centered between them instead. The bar does the same for its width.
        window_bounds: Some(WindowBounds::Windowed(Bounds {
          origin: point(px(0.), px(0.)),
          size: Size::new(px(0.), px(0.)),
        })),
        ..Default::default()
      },
      |window, cx| {
        let view = cx.new(|_| Panel {
          height: Anim::new(0.),
        });
        cx.new(|cx| {
          Root::new(view, window, cx)
            .bordered(false)
            .bg(gpui_kit::transparent_black())
        })
      },
    )
  }
}
