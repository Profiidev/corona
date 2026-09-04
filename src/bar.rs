use gpui_kit::{
  App, Bounds, Size, Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
  component::{button::Button, status_bar::StatusBar, *},
  layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
  point,
  prelude::*,
  px,
};

pub struct Bar;

impl Bar {
  pub fn create(cx: &mut App) -> Result<gpui_kit::WindowHandle<Root>, anyhow::Error> {
    cx.open_window(
      WindowOptions {
        kind: WindowKind::LayerShell(LayerShellOptions {
          anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
          exclusive_zone: Some(px(30.)),
          exclusive_edge: None,
          margin: None,
          layer: Layer::Top,
          namespace: "corona_bar".to_string(),
          keyboard_interactivity: KeyboardInteractivity::OnDemand,
        }),
        window_background: WindowBackgroundAppearance::Transparent,
        app_id: Some("corona_bar".to_string()),
        titlebar: None,
        window_bounds: Some(WindowBounds::Windowed(Bounds {
          origin: point(px(0.), px(0.)),
          size: Size::new(px(0.), px(30.)),
        })),
        ..Default::default()
      },
      |window, cx| {
        let view = cx.new(|_| Bar);
        cx.new(|cx| Root::new(view, window, cx).bordered(false))
      },
    )
  }
}

impl Render for Bar {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    StatusBar::new()
      .size_full()
      .child(Button::new("id").label("label"))
  }
}
