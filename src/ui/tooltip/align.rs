use gpui_kit::{Bounds, Pixels, Size};

use crate::config::placement::{Placement, PlacmentBounds};

const TOOLTIP_GAP: f32 = 4.;

#[derive(PartialEq, Clone, Copy)]
pub struct Align {
  pub along: f32,
  pub across: f32,
}

impl Align {
  pub fn from_bounds(
    anchor: Bounds<Pixels>,
    bar_bounds: Bounds<Pixels>,
    size: Size<Pixels>,
    placement: Placement,
  ) -> Self {
    let (bar_len, _) = bar_bounds.extent_p(placement);
    let width = if placement.is_horizontal() {
      size.width.as_f32()
    } else {
      size.height.as_f32()
    };

    let center = placement.along(anchor.center()).as_f32();
    let along = (center - width / 2.).clamp(
      TOOLTIP_GAP,
      (bar_len.as_f32() - width - TOOLTIP_GAP).max(TOOLTIP_GAP),
    );

    Self {
      along,
      across: TOOLTIP_GAP,
    }
  }
}
