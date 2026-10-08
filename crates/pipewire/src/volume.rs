pub fn to_slider(linear: f32) -> f32 {
  linear.cbrt()
}

pub fn to_linear(slider: f32) -> f32 {
  slider.powi(3)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn cubic_slider() {
    assert_eq!((to_slider(0.), to_slider(1.)), (0., 1.));
    assert_eq!((to_linear(0.), to_linear(1.)), (0., 1.));
    assert_eq!(to_linear(0.5), 0.125);
    assert!((to_slider(0.125) - 0.5).abs() < 1e-6);
    for i in 0..=150 {
      let slider = i as f32 / 100.;
      assert!(
        (to_slider(to_linear(slider)) - slider).abs() < 1e-5,
        "{slider}"
      );
    }
    // amplified past 100% stays above 1, nothing is clamped here
    assert!(to_slider(2.) > 1.);
    assert!(to_slider(-1.) < 0.);
  }

  #[test]
  fn cubic_slider_boundary_values() {
    assert!(to_slider(f32::NAN).is_nan());
    assert!(to_linear(f32::NAN).is_nan());
    assert_eq!(to_slider(f32::INFINITY), f32::INFINITY);
    assert_eq!(to_linear(f32::INFINITY), f32::INFINITY);
    assert_eq!(to_slider(f32::NEG_INFINITY), f32::NEG_INFINITY);
    assert_eq!(to_linear(f32::NEG_INFINITY), f32::NEG_INFINITY);
    assert!(to_slider(f32::MIN_POSITIVE) > 0.0);
    assert_eq!(to_linear(f32::MIN_POSITIVE), 0.0);
    assert!(to_linear(1e-10) > 0.0);
  }
}
