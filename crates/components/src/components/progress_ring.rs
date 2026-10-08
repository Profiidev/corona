use std::f32::consts::TAU;

use gpui_kit::{
  Bounds, Hsla, IntoElement, Styled, canvas,
  component::plot::shape::{Arc, ArcData},
  fill, point, px, size,
};

pub fn ring(progress: f32, color: Hsla, width: f32) -> impl IntoElement {
  canvas(
    |_, _, _| {},
    move |bounds, _, window, _| {
      let radius = (bounds.size.width.min(bounds.size.height).as_f32() - width) / 2.;
      let arc = Arc::new()
        .inner_radius(radius - width / 2.)
        .outer_radius(radius + width / 2.);
      arc.paint(
        &ArcData::new(&(), 0, 1., 0., TAU),
        color.opacity(0.2),
        &bounds,
        window,
      );
      if progress <= 0. {
        return;
      }
      let end = progress * TAU;
      arc.paint(
        &ArcData::new(&(), 1, progress, 0., end),
        color,
        &bounds,
        window,
      );

      let center = bounds.center();
      for angle in [0., end] {
        let dot = point(
          center.x + px(radius * angle.sin() - width / 2.),
          center.y - px(radius * angle.cos() + width / 2.),
        );
        window.paint_quad(
          fill(Bounds::new(dot, size(px(width), px(width))), color).corner_radii(px(width / 2.)),
        );
      }
    },
  )
  .absolute()
  .size_full()
}

#[cfg(test)]
mod tests {
  use gpui_kit::{self as gpui, ParentElement, TestAppContext, div, red};

  use super::*;
  use crate::test_view;

  #[gpui::test]
  fn paints_any_progress(cx: &mut TestAppContext) {
    let (handle, _) = test_view::open(cx, |_, _| {
      div()
        .children(
          [-1., 0., 0.5, 1., 2.].map(|p| div().relative().size(px(40.)).child(ring(p, red(), 4.))),
        )
        // thicker than the ring is wide
        .child(div().relative().size(px(4.)).child(ring(0.5, red(), 10.)))
        .into_any_element()
    });
    test_view::draw(handle, cx);
  }
}
