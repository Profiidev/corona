use std::time::{Duration, Instant};

use gpui_kit::base::animation::{ease_in_cubic, ease_out_cubic};

pub struct SmoothRetarget {
  from: f32,
  to: f32,
  start: Instant,
  dur: Duration,
}

impl SmoothRetarget {
  pub fn new(value: f32) -> Self {
    Self {
      from: value,
      to: value,
      start: Instant::now(),
      dur: Duration::ZERO,
    }
  }

  pub fn value(&self) -> (f32, bool) {
    if self.dur.is_zero() {
      return (self.to, false);
    }
    let t = (self.start.elapsed().as_secs_f32() / self.dur.as_secs_f32()).min(1.);
    let eased = if self.to >= self.from {
      ease_out_cubic(t)
    } else {
      ease_in_cubic(t)
    };
    (self.from + (self.to - self.from) * eased, t < 1.)
  }

  pub fn retarget(&mut self, to: f32, speed: Duration) {
    if self.to == to {
      return;
    }
    self.from = self.value().0;
    self.to = to;
    self.dur = speed.mul_f32((to - self.from).abs());
    self.start = Instant::now();
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Moves the start back, so `value` reads `t` of the way through
  fn at(anim: &mut SmoothRetarget, t: f32) {
    anim.start = Instant::now()
      .checked_sub(anim.dur.mul_f32(t))
      .expect("monotonic clock too young");
  }

  fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
  }

  #[test]
  fn new_is_at_rest() {
    let anim = SmoothRetarget::new(0.4);
    assert_eq!(anim.value(), (0.4, false));
  }

  #[test]
  fn same_target_is_a_no_op() {
    let mut anim = SmoothRetarget::new(1.);
    anim.retarget(1., Duration::from_secs(100));
    assert_eq!(anim.value(), (1., false));
    assert!(anim.dur.is_zero());
  }

  #[test]
  fn duration_scales_with_distance() {
    let mut anim = SmoothRetarget::new(1.);
    anim.retarget(3., Duration::from_secs(10));
    assert_eq!(anim.dur, Duration::from_secs(20));
    let mut anim = SmoothRetarget::new(1.);
    anim.retarget(0.5, Duration::from_secs(10));
    assert_eq!(anim.dur, Duration::from_secs(5));
  }

  #[test]
  fn zero_speed_jumps() {
    let mut anim = SmoothRetarget::new(0.);
    anim.retarget(1., Duration::ZERO);
    assert_eq!(anim.value(), (1., false));
  }

  #[test]
  fn starts_at_the_old_value() {
    let mut anim = SmoothRetarget::new(0.);
    anim.retarget(1., Duration::from_secs(1000));
    let (value, moving) = anim.value();
    assert!(close(value, 0.), "{value}");
    assert!(moving);
  }

  #[test]
  fn rising_eases_out_falling_eases_in() {
    let mut up = SmoothRetarget::new(0.);
    up.retarget(1., Duration::from_secs(10));
    at(&mut up, 0.5);
    // ease out is past halfway at half time, ease in short of it
    assert!(close(up.value().0, ease_out_cubic(0.5)));
    assert!(up.value().0 > 0.5);

    let mut down = SmoothRetarget::new(1.);
    down.retarget(0., Duration::from_secs(10));
    at(&mut down, 0.5);
    assert!(close(down.value().0, 1. - ease_in_cubic(0.5)));
    assert!(down.value().0 > 0.5);
  }

  #[test]
  fn clamps_once_done() {
    let mut anim = SmoothRetarget::new(0.);
    anim.retarget(2., Duration::from_secs(1));
    at(&mut anim, 3.);
    assert_eq!(anim.value(), (2., false));
  }

  #[test]
  fn retarget_mid_flight_continues_from_the_current_value() {
    let mut anim = SmoothRetarget::new(0.);
    anim.retarget(1., Duration::from_secs(10));
    at(&mut anim, 0.5);
    let mid = anim.value().0;
    anim.retarget(0., Duration::from_secs(10));
    assert!(close(anim.from, mid));
    assert!(close(anim.value().0, mid));
    assert_eq!(anim.dur, Duration::from_secs(10).mul_f32(anim.from));
  }
}
