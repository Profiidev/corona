use std::{cell::Cell, rc::Rc};

use gpui_kit::{
  App, Bounds, Size, Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
  component::{button::Button, status_bar::StatusBar, *},
  layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
  point,
  prelude::*,
  px,
};

use crate::{lock::Lock, panel::PanelState};

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
  fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
    let button_bounds = Rc::new(Cell::new(Bounds::default()));
    let bar_bounds = Rc::new(Cell::new(Bounds::default()));

    StatusBar::new()
      .size_full()
      .on_prepaint({
        let bar_bounds = bar_bounds.clone();
        move |bounds, _, _| {
          bar_bounds.set(bounds);
        }
      })
      .child(
        Button::new("id")
          .label("label")
          .on_prepaint({
            let button_bounds = button_bounds.clone();
            move |bounds, _, _| {
              button_bounds.set(bounds);
            }
          })
          .on_click(move |_, _, cx| {
            let button_bounds = button_bounds.get();
            let bar_bounds = bar_bounds.get();
            if PanelState::is_open("test", cx) {
              PanelState::close("test", cx);
            } else {
              PanelState::open("test".into(), button_bounds, bar_bounds, cx)
                .expect("failed to open panel");
            }
          }),
      )
      // Only here to exercise the session-lock patch; Escape unlocks, nothing is checked.
      .child(
        Button::new("lock")
          .label("lock")
          .ml_auto()
          .on_click(|_, _, cx| Lock::lock(cx)),
      )
  }
}
