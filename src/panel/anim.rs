use std::time::{Duration, Instant};

pub struct Anim {
  from: f32,
  to: f32,
  start: Instant,
  speed: Duration,
}

impl Anim {
  pub fn new(value: f32, speed: Duration) -> Self {
    Self {
      from: value,
      to: value,
      start: Instant::now(),
      speed,
    }
  }

  /// Current value, and whether the animation is still running.
  pub fn value(&self) -> (f32, bool) {
    let t = (self.start.elapsed().as_secs_f32() / self.speed.as_secs_f32()).min(1.);
    let eased = 1. - (1. - t) * (1. - t); // ease-out-quad, as in panel.slint
    (self.from + (self.to - self.from) * eased, t < 1.)
  }

  pub fn retarget(&mut self, to: f32) {
    if self.to == to {
      return;
    }
    self.from = self.value().0;
    self.to = to;
    self.start = Instant::now();
  }

  pub fn speed(&self) -> Duration {
    self.speed
  }
}
