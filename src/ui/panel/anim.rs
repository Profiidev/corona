use std::time::{Duration, Instant};

use gpui_kit::base::animation::{ease_in_cubic, ease_out_cubic};

pub struct Anim {
  from: f32,
  to: f32,
  start: Instant,
  dur: Duration,
}

impl Anim {
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
