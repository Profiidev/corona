use std::{cell::Cell, rc::Rc};

use gpui_kit::{
  App, Bounds, Path, PathBuilder, Pixels, Size, Window, WindowBackgroundAppearance, WindowBounds,
  WindowKind, WindowOptions, canvas,
  component::{ActiveTheme, button::Button, status_bar::StatusBar, *},
  div,
  layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
  point,
  prelude::*,
  px,
};

use crate::panel::PanelState;

/// The bar proper. Its corner flares hang below this, over whatever is underneath.
const HEIGHT: f32 = 30.;

pub struct Bar;

impl Bar {
  pub fn create(cx: &mut App) -> Result<gpui_kit::WindowHandle<Root>, anyhow::Error> {
    let flare = cx.theme().radius_2xl().as_f32();

    cx.open_window(
      WindowOptions {
        kind: WindowKind::LayerShell(LayerShellOptions {
          anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
          exclusive_zone: Some(px(HEIGHT)),
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
          size: Size::new(px(0.), px(HEIGHT + flare)),
        })),
        ..Default::default()
      },
      |window, cx| {
        let view = cx.new(|_| Bar);
        cx.new(|cx| {
          Root::new(view, window, cx)
            .bordered(false)
            .bg(gpui_kit::transparent_black())
        })
      },
    )
  }
}

impl Render for Bar {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();
    let bg = theme.tokens.status_bar;
    let flare = theme.radius_2xl();

    window.set_input_region(Some(&[Bounds {
      origin: point(px(0.), px(0.)),
      size: Size::new(window.viewport_size().width, px(HEIGHT)),
    }]));

    let bar_bounds = Rc::new(Cell::new(Bounds::default()));

    div()
      .size_full()
      .relative()
      .child(
        canvas(
          |_, _, _| (),
          move |bounds, _, window, _| {
            if let Some(path) = bar_path(bounds, flare) {
              window.paint_path(path, bg);
            }
          },
        )
        .absolute()
        .size_full()
        .inset_0(),
      )
      .child(
        StatusBar::new()
          .absolute()
          .top_0()
          .left_0()
          .right_0()
          .h(px(HEIGHT))
          .border_0()
          .on_prepaint({
            let bar_bounds = bar_bounds.clone();
            move |bounds, _, _| {
              bar_bounds.set(bounds);
            }
          })
          .children((0..3).map(|i| {
            let button_bounds = Rc::new(Cell::new(Bounds::default()));

            Button::new(format!("Button {}", i + 1))
              .label("label")
              .on_prepaint({
                let button_bounds = button_bounds.clone();
                move |bounds, _, _| {
                  button_bounds.set(bounds);
                }
              })
              .when(i != 0, |b| b.ml_auto())
              .on_click({
                let bar_bounds = bar_bounds.clone();
                move |_, _, cx| {
                  let button_bounds = button_bounds.get();
                  let bar_bounds = bar_bounds.get();
                  if PanelState::is_open("test", cx) {
                    PanelState::close("test", cx);
                  } else {
                    PanelState::open("test".into(), button_bounds, bar_bounds, cx)
                      .expect("failed to open panel");
                  }
                }
              })
          })),
      )
  }
}

fn bar_path(bounds: Bounds<Pixels>, n: Pixels) -> Option<Path<Pixels>> {
  let (l, r, b, t) = (bounds.left(), bounds.right(), bounds.bottom(), bounds.top());

  let mut p = PathBuilder::fill();
  p.move_to(point(l, t));
  p.line_to(point(r, t));
  p.line_to(point(r, b));
  p.arc_to(point(n, n), px(0.), false, false, point(r - n, b - n));
  p.line_to(point(l + n, b - n));
  p.arc_to(point(n, n), px(0.), false, false, point(l, b));
  p.close();
  p.build().ok()
}
