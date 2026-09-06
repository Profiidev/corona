use std::{cell::Cell, rc::Rc};

use gpui_kit::{
  App, Bounds, Path, PathBuilder, Pixels, Window, WindowBackgroundAppearance, WindowBounds,
  WindowKind, WindowOptions, canvas,
  component::{ActiveTheme, button::Button, *},
  div,
  layer_shell::{KeyboardInteractivity, Layer, LayerShellOptions},
  point,
  prelude::*,
  px,
};

use crate::{
  APP_NAME,
  bar::placement::PlacmentBounds,
  panel::{ControlPanel, PanelState},
};

mod placement;

pub use placement::Placement;

const BAR_NAMESPACE: &str = "corona-bar";

/// The bar proper. Its corner flares hang below this, over whatever is underneath.
const HEIGHT: f32 = 30.;

pub struct Bar {
  placement: Placement,
}

impl Bar {
  pub fn create(
    cx: &mut App,
    placement: Placement,
  ) -> Result<gpui_kit::WindowHandle<Root>, anyhow::Error> {
    let flare = cx.theme().radius_2xl().as_f32();

    cx.open_window(
      WindowOptions {
        kind: WindowKind::LayerShell(LayerShellOptions {
          anchor: placement.anchor(),
          exclusive_zone: Some(px(HEIGHT)),
          exclusive_edge: None,
          margin: None,
          layer: Layer::Top,
          namespace: BAR_NAMESPACE.to_string(),
          keyboard_interactivity: KeyboardInteractivity::OnDemand,
        }),
        window_background: WindowBackgroundAppearance::Transparent,
        app_id: Some(APP_NAME.to_string()),
        titlebar: None,
        window_bounds: Some(WindowBounds::Windowed(Bounds {
          origin: point(px(0.), px(0.)),
          size: placement.size(HEIGHT + flare, 0.),
        })),
        ..Default::default()
      },
      |window, cx| {
        let view = cx.new(|_| Bar { placement });
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
      size: self
        .placement
        .size(HEIGHT, window.viewport_size().width.as_f32()),
    }]));

    let bar_bounds = Rc::new(Cell::new(Bounds::default()));

    div()
      .size_full()
      .relative()
      .child(
        canvas(|_, _, _| (), {
          let placement = self.placement;
          move |bounds, _, window, _| {
            if let Some(path) = bar_path(bounds, flare, placement) {
              window.paint_path(path, bg);
            }
          }
        })
        .absolute()
        .size_full()
        .inset_0(),
      )
      .child(
        div()
          .absolute()
          .flex()
          .items_center()
          .gap_2()
          .when(self.placement != Placement::Bottom, |b| b.top_0())
          .when(self.placement != Placement::Top, |b| b.bottom_0())
          .when(self.placement != Placement::Left, |b| b.left_0())
          .when(self.placement != Placement::Right, |b| b.right_0())
          .when(self.placement.is_horizontal(), |b| {
            b.flex_row().h(px(HEIGHT))
          })
          .when(self.placement.is_vertical(), |b| b.flex_col().w(px(HEIGHT)))
          .on_prepaint({
            let bar_bounds = bar_bounds.clone();
            move |bounds, _, _| {
              bar_bounds.set(bounds);
            }
          })
          .children((0..3).map(|i| {
            let placement = self.placement;
            let button_bounds = Rc::new(Cell::new(Bounds::default()));

            Button::new(format!("Button {}", i + 1))
              .label("p")
              .on_prepaint({
                let button_bounds = button_bounds.clone();
                move |bounds, _, _| {
                  button_bounds.set(bounds);
                }
              })
              .when(i != 0 && placement.is_horizontal(), |b| b.ml_auto())
              .when(i != 0 && placement.is_vertical(), |b| b.mt_auto())
              .on_click({
                let bar_bounds = bar_bounds.clone();
                move |_, _, cx| {
                  let button_bounds = button_bounds.get();
                  let bar_bounds = bar_bounds.get();
                  PanelState::toggle(ControlPanel, button_bounds, bar_bounds, cx)
                    .expect("failed to open panel");
                }
              })
          })),
      )
  }
}

fn bar_path(bounds: Bounds<Pixels>, n: Pixels, placement: Placement) -> Option<Path<Pixels>> {
  let (len, depth) = bounds.extent_p(placement);
  let at = |along, across| bounds.point_p(placement, along, across);
  // Mirrored placements reverse the plane, so the arcs sweep the other way.
  let sweep = placement.mirrored();
  let z = px(0.);

  let mut p = PathBuilder::fill();
  p.move_to(at(z, z));
  p.line_to(at(len, z));
  p.line_to(at(len, depth));
  p.arc_to(point(n, n), px(0.), false, sweep, at(len - n, depth - n));
  p.line_to(at(n, depth - n));
  p.arc_to(point(n, n), px(0.), false, sweep, at(z, depth));
  p.close();
  p.build().ok()
}
