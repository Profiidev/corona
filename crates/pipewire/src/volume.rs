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
}
