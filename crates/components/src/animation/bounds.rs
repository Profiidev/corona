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

#[cfg(test)]
mod tests {
  use super::*;

  const LONG: Duration = Duration::from_secs(1000);

  fn rect(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
    Bounds {
      origin: point(px(x), px(y)),
      size: Size::new(px(w), px(h)),
    }
  }

  #[test]
  fn first_target_jumps() {
    let mut anim = BoundsAnimation::default();
    assert_eq!(
      anim.step(Some(rect(1., 2., 3., 4.)), LONG),
      (Some(rect(1., 2., 3., 4.)), false)
    );
  }

  #[test]
  fn none_stays_none() {
    let mut anim = BoundsAnimation::default();
    assert_eq!(anim.step(None, LONG), (None, false));
  }

  #[test]
  fn zero_duration_jumps() {
    let mut anim = BoundsAnimation::default();
    anim.step(Some(rect(0., 0., 1., 1.)), Duration::ZERO);
    let b = rect(10., 10., 5., 5.);
    assert_eq!(anim.step(Some(b), Duration::ZERO), (Some(b), false));
  }

  #[test]
  fn new_target_starts_at_the_shown_bounds() {
    let mut anim = BoundsAnimation::default();
    let a = rect(0., 0., 10., 10.);
    anim.step(Some(a), LONG);
    let (shown, moving) = anim.step(Some(rect(100., 100., 50., 50.)), LONG);
    assert!(moving);
    let shown = shown.unwrap();
    assert!(shown.origin.x.as_f32() < 1. && shown.size.width.as_f32() < 11.);
    // the same target again does not restart it
    let start = anim.from.unwrap().1;
    anim.step(Some(rect(100., 100., 50., 50.)), LONG);
    assert_eq!(anim.from.unwrap().1, start);
  }

  #[test]
  fn hidden_in_between_jumps() {
    let mut anim = BoundsAnimation::default();
    anim.step(Some(rect(0., 0., 1., 1.)), LONG);
    assert_eq!(anim.step(None, LONG), (None, false));
    let b = rect(5., 5., 5., 5.);
    assert_eq!(anim.step(Some(b), LONG), (Some(b), false));
  }

  #[test]
  fn interpolates_every_field_and_clamps() {
    let mut anim = BoundsAnimation::default();
    let (a, b) = (rect(0., 10., 20., 30.), rect(100., 110., 120., 130.));
    let dur = Duration::from_secs(10);
    anim.step(Some(a), dur);
    anim.step(Some(b), dur);

    let half = Instant::now().checked_sub(dur / 2).unwrap();
    anim.from = Some((a, half));
    let (shown, moving) = anim.step(Some(b), dur);
    assert!(moving);
    let shown = shown.unwrap();
    let e = ease_out_cubic(0.5) * 100.;
    for (got, base) in [
      (shown.origin.x, 0.),
      (shown.origin.y, 10.),
      (shown.size.width, 20.),
      (shown.size.height, 30.),
    ] {
      assert!((got.as_f32() - base - e).abs() < 0.1, "{got:?}");
    }

    anim.from = Some((a, Instant::now().checked_sub(dur * 3).unwrap()));
    assert_eq!(anim.step(Some(b), dur), (Some(b), false));
  }

  #[test]
  fn zero_duration_mid_flight_snaps() {
    let mut anim = BoundsAnimation::default();
    let b = rect(100., 100., 50., 50.);
    anim.step(Some(rect(0., 0., 1., 1.)), LONG);
    anim.step(Some(b), LONG);
    let (shown, moving) = anim.step(Some(b), Duration::ZERO);
    assert_eq!(shown, Some(b));
    assert!(!moving);
  }
}
