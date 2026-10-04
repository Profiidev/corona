use gpui_kit::{Bounds, Pixels, Window};

#[derive(Default)]
pub struct InputRegion(Option<Bounds<Pixels>>);

impl InputRegion {
  pub fn set(&mut self, region: Bounds<Pixels>, window: &mut Window) {
    if self.0 != Some(region) {
      self.0 = Some(region);
      window.set_input_region(Some(&[region]));
    }
  }
}
