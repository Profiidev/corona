use std::time::Duration;

use crate::animation::smooth_retarget::SmoothRetarget;

pub struct Glide<const N: usize> {
  from: [f32; N],
  to: Option<[f32; N]>,
  anim: SmoothRetarget,
}

impl<const N: usize> Default for Glide<N> {
  fn default() -> Self {
    Self {
      from: [0.; N],
      to: None,
      anim: SmoothRetarget::new(1.),
    }
  }
}

impl<const N: usize> Glide<N> {
  pub fn reset(&mut self) {
    self.to = None;
  }

  pub fn value(&mut self, target: [f32; N], duration: Duration) -> ([f32; N], bool) {
    let Some(to) = self.to else {
      *self = Self {
        from: target,
        to: Some(target),
        anim: SmoothRetarget::new(1.),
      };
      return (target, false);
    };
    let (progress, moving) = self.anim.value();
    let current = std::array::from_fn(|i| self.from[i] + (to[i] - self.from[i]) * progress);
    if to == target {
      return (current, moving);
    }
    self.from = current;
    self.to = Some(target);
    self.anim = SmoothRetarget::new(0.);
    self.anim.retarget(1., duration);
    (current, true)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn first_target_is_taken_and_reset_jumps() {
    let mut glide = Glide::<2>::default();
    assert_eq!(
      glide.value([1., 2.], Duration::from_secs(10)),
      ([1., 2.], false)
    );
    // A new target starts gliding from the current values.
    assert_eq!(
      glide.value([5., 6.], Duration::from_secs(10)),
      ([1., 2.], true)
    );
    glide.reset();
    assert_eq!(
      glide.value([7., 8.], Duration::from_secs(10)),
      ([7., 8.], false)
    );
    assert_eq!(glide.value([9., 9.], Duration::ZERO).0, [7., 8.]);
  }

  #[test]
  fn holds_still_on_the_same_target() {
    let mut glide = Glide::<1>::default();
    glide.value([3.], Duration::from_secs(10));
    for _ in 0..3 {
      assert_eq!(glide.value([3.], Duration::from_secs(10)), ([3.], false));
    }
  }

  #[test]
  fn reports_moving_once_then_settles() {
    let mut glide = Glide::<1>::default();
    glide.value([0.], Duration::ZERO);
    // the frame that retargets still shows the old value, and asks for another
    assert_eq!(glide.value([4.], Duration::ZERO), ([0.], true));
    assert_eq!(glide.value([4.], Duration::ZERO), ([4.], false));
  }

  #[test]
  fn retarget_mid_flight_starts_from_the_current_value() {
    let long = Duration::from_secs(1000);
    let mut glide = Glide::<2>::default();
    glide.value([0., 0.], long);
    glide.value([10., -10.], long);
    let (value, moving) = glide.value([20., 20.], long);
    assert!(moving);
    assert!(value.iter().all(|v| v.abs() < 0.1), "{value:?}");
    assert_eq!(glide.to, Some([20., 20.]));
    assert!(glide.from.iter().all(|v| v.abs() < 0.1));
  }
}
