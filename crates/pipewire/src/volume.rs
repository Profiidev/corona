pub fn to_slider(linear: f32) -> f32 {
  linear.cbrt()
}

pub fn to_linear(slider: f32) -> f32 {
  slider.powi(3)
}
