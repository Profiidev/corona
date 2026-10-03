use std::time::{Duration, Instant};

use gpui_kit::{Bounds, Pixels, Size, base::animation::ease_out_cubic, point, px};

#[derive(Default)]
pub struct BoundsAnimation {
  target: Option<Bounds<Pixels>>,
  shown: Option<Bounds<Pixels>>,
  from: Option<(Bounds<Pixels>, Instant)>,
}

impl BoundsAnimation {
  pub fn step(
    &mut self,
    target: Option<Bounds<Pixels>>,
    duration: Duration,
  ) -> (Option<Bounds<Pixels>>, bool) {
    if target != self.target {
      self.from = match (self.shown, target) {
        (Some(shown), Some(_)) if !duration.is_zero() => Some((shown, Instant::now())),
        _ => None,
      };
      self.target = target;
    }

    let (shown, moving) = match (self.target, self.from) {
      (Some(to), Some((from, start))) => {
        let t = (start.elapsed().as_secs_f32() / duration.as_secs_f32()).min(1.);
        let e = ease_out_cubic(t);
        let lerp = |a: Pixels, b: Pixels| px(a.as_f32() + (b - a).as_f32() * e);
        let b = Bounds {
          origin: point(
            lerp(from.origin.x, to.origin.x),
            lerp(from.origin.y, to.origin.y),
          ),
          size: Size::new(
            lerp(from.size.width, to.size.width),
            lerp(from.size.height, to.size.height),
          ),
        };
        (Some(b), t < 1.)
      }
      (to, _) => (to, false),
    };
    self.shown = shown;
    (shown, moving)
  }
}
