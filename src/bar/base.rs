use std::{cell::Cell, rc::Rc};

use gpui_kit::{
  Bounds, Path, PathBuilder, Pixels, Window, canvas,
  component::{ActiveTheme, button::Button, *},
  div, point,
  prelude::*,
  px,
};

use crate::{
  config::placement::{Placement, PlacementStyle, PlacmentBounds},
  panel::{ControlPanel, PanelState},
};

pub struct Bar {
  placement: Placement,
  height: f32,
}

impl Bar {
  pub fn new(placement: Placement, height: f32) -> Self {
    Self { placement, height }
  }
}

impl Render for Bar {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();
    let bg = theme.tokens.status_bar;
    let flare = theme.radius_2xl();

    let viewport = window.viewport_size();
    let len = self.placement.len(viewport);
    window.set_input_region(Some(&[self.placement.rect(
      viewport,
      px(0.),
      len,
      px(self.height),
    )]));

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
          .anchor_p(self.placement)
          .along_start_p(self.placement)
          .along_end_p(self.placement)
          .overflow_hidden()
          .when(self.placement.is_horizontal(), |b| {
            b.flex_row().h(px(self.height))
          })
          .when(self.placement.is_vertical(), |b| {
            b.flex_col().w(px(self.height))
          })
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
                  PanelState::toggle(ControlPanel, button_bounds, bar_bounds, placement, cx)
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
