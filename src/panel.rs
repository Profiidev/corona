use gpui_kit::{
  App, AppContext, Bounds, Render, Size, Window, WindowBackgroundAppearance, WindowBounds,
  WindowKind, WindowOptions,
  component::{button::Button, *},
  layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
  point,
  prelude::*,
  px,
};

pub struct Panel;

impl Panel {
  pub fn create(cx: &mut App) -> Result<gpui_kit::WindowHandle<Root>, anyhow::Error> {
    cx.open_window(
      WindowOptions {
        kind: WindowKind::LayerShell(LayerShellOptions {
          anchor: Anchor::TOP | Anchor::LEFT,
          exclusive_zone: None,
          exclusive_edge: None,
          margin: None,
          layer: Layer::Overlay,
          namespace: "corona_panel".to_string(),
          keyboard_interactivity: KeyboardInteractivity::OnDemand,
        }),
        window_background: WindowBackgroundAppearance::Transparent,
        app_id: Some("corona_bar".to_string()),
        titlebar: None,
        window_bounds: Some(WindowBounds::Windowed(Bounds {
          origin: point(px(0.), px(0.)),
          size: Size::new(px(200.), px(200.)),
        })),
        ..Default::default()
      },
      |window, cx| {
        let view = cx.new(|_| Panel);
        cx.new(|cx| Root::new(view, window, cx).bordered(false))
      },
    )
  }
}

impl Render for Panel {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    Button::new("test").label("test")
  }
}
