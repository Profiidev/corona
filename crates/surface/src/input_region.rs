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

#[cfg(test)]
mod tests {
  use gpui_kit::{AppContext, TestAppContext, point, px, size};

  use super::*;
  use crate::test_support::{self, plain_window};

  #[gpui_kit::test]
  fn keeps_the_last_region(cx: &mut TestAppContext) {
    test_support::setup(cx);
    let handle = plain_window(cx);
    let a = Bounds::new(point(px(0.), px(0.)), size(px(10.), px(10.)));
    let b = Bounds::new(point(px(5.), px(0.)), size(px(10.), px(10.)));
    cx.update_window(handle, |_, window, _| {
      let mut region = InputRegion::default();
      assert_eq!(region.0, None);
      region.set(a, window);
      region.set(a, window);
      assert_eq!(region.0, Some(a));
      region.set(b, window);
      assert_eq!(region.0, Some(b));
    })
    .unwrap();
  }
}
