use gpui_kit::{App, Bounds, Pixels, component::ActiveTheme};

use crate::{
  bar::{Placement, PlacmentBounds},
  panel::style::PanelStyle,
};

#[derive(PartialEq, Clone, Copy)]
pub enum Align {
  Left,
  Relative(f32),
  Right,
}

impl Align {
  pub fn from_bounds(
    button_bounds: Bounds<Pixels>,
    bar_bounds: Bounds<Pixels>,
    width: f32,
    placement: Placement,
    cx: &mut App,
  ) -> Self {
    let theme = cx.theme();
    let notch = theme.panel_radius();

    let button_center = placement.along(button_bounds.center()).as_f32();
    let total_width = bar_bounds.extent_p(placement).0.as_f32();
    let half_width = width / 2. + notch;

    if button_center < half_width {
      Align::Left
    } else if button_center > total_width - half_width {
      Align::Right
    } else {
      Align::Relative(button_center)
    }
  }
}
