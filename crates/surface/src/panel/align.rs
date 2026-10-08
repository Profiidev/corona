use corona_config::placement::{Placement, PlacmentBounds};
use gpui_kit::{App, Bounds, Pixels, component::ActiveTheme};

use crate::panel::style::PanelStyle;

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
    Self::with_notch(
      button_bounds,
      bar_bounds,
      width,
      placement,
      cx.theme().panel_radius(),
    )
  }

  fn with_notch(
    button_bounds: Bounds<Pixels>,
    bar_bounds: Bounds<Pixels>,
    width: f32,
    placement: Placement,
    notch: f32,
  ) -> Self {
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

#[cfg(test)]
mod tests {
  use gpui_kit::{point, px, size};

  use super::*;

  fn rect(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
    Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
  }

  /// A 2px button centered at `at` along a 1000px bar
  fn align(at: f32, placement: Placement) -> Align {
    let (button, bar) = match placement.is_horizontal() {
      true => (rect(at - 1., 0., 2., 30.), rect(0., 0., 1000., 30.)),
      false => (rect(0., at - 1., 30., 2.), rect(0., 0., 30., 1000.)),
    };
    // half width: 100 / 2 + 10 = 60
    Align::with_notch(button, bar, 100., placement, 10.)
  }

  #[test]
  fn edges_snap_and_the_middle_follows_the_button() {
    for placement in [
      Placement::Top,
      Placement::Bottom,
      Placement::Left,
      Placement::Right,
    ] {
      assert!(align(0., placement) == Align::Left);
      assert!(align(59., placement) == Align::Left);
      // the boundaries themselves stay relative
      assert!(align(60., placement) == Align::Relative(60.));
      assert!(align(500., placement) == Align::Relative(500.));
      assert!(align(940., placement) == Align::Relative(940.));
      assert!(align(941., placement) == Align::Right);
      assert!(align(1000., placement) == Align::Right);
    }
  }

  #[test]
  fn vertical_bars_measure_along_y() {
    let button = rect(500., 10., 2., 2.);
    let bar = rect(0., 0., 1000., 1000.);
    assert!(Align::with_notch(button, bar, 100., Placement::Left, 10.) == Align::Left);
    assert!(Align::with_notch(button, bar, 100., Placement::Top, 10.) == Align::Relative(501.));
  }

  #[test]
  fn a_panel_wider_than_the_bar_snaps_left() {
    let bar = rect(0., 0., 100., 30.);
    let button = rect(49., 0., 2., 30.);
    assert!(Align::with_notch(button, bar, 400., Placement::Top, 0.) == Align::Left);
  }
}
