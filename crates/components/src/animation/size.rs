use std::time::{Duration, Instant};

use gpui_kit::{
  Animation, AnimationExt, AnyElement, App, Axis, IntoElement, Styled, ease_out_quint, px,
};

use corona_config::ConfigProvider;

pub struct SizeAnimation {
  current_target: f32,
  from: Option<(f32, Instant)>,
  generation: usize,
  duration: Duration,
}

impl SizeAnimation {
  pub fn new(duration: Duration) -> Self {
    Self {
      current_target: 0.0,
      from: None,
      generation: 0,
      duration,
    }
  }

  pub fn start(mut self, start: f32) -> Self {
    self.current_target = start;
    self
  }

  fn duration(&self, cx: &App) -> Duration {
    cx.config().shell.animation.duration(self.duration)
  }

  pub fn reset(&mut self) {
    self.current_target = 0.0;
    self.from = None;
  }

  pub fn animate<E>(
    &mut self,
    id: &'static str,
    axis: Axis,
    target: f32,
    cx: &App,
    element: E,
  ) -> AnyElement
  where
    E: Styled + IntoElement + 'static,
  {
    if (self.current_target - target).abs() > f32::EPSILON {
      self.from = Some((self.current_target, Instant::now()));
      self.current_target = target;
      self.generation = self.generation.wrapping_add(1);
    }

    let size = move |element: E, size: f32| match axis {
      Axis::Horizontal => element.w(px(size)),
      Axis::Vertical => element.h(px(size)),
    };

    match self.from {
      Some((from, at)) if at.elapsed() < self.duration(cx) => element
        .with_animation(
          (id, self.generation),
          Animation::new(self.duration(cx)).with_easing(ease_out_quint()),
          move |element, delta| size(element, from + (target - from) * delta),
        )
        .into_any_element(),
      _ => size(element, target).into_any_element(),
    }
  }
}

#[cfg(test)]
mod tests {
  use corona_config::Config;
  use gpui_kit::{self as gpui, TestAppContext, div};

  use super::*;

  #[test]
  fn start_and_reset() {
    let anim = SizeAnimation::new(Duration::from_secs(1)).start(5.);
    assert_eq!(anim.current_target, 5.);
    assert!(anim.from.is_none());
    let mut anim = anim;
    anim.from = Some((1., Instant::now()));
    anim.reset();
    assert_eq!(anim.current_target, 0.);
    assert!(anim.from.is_none());
  }

  #[gpui::test]
  fn animate_restarts_only_on_a_new_target(cx: &mut TestAppContext) {
    cx.update(|cx| {
      cx.set_global(Config::default());
      let mut anim = SizeAnimation::new(Duration::from_secs(1)).start(10.);
      let _ = anim.animate("a", Axis::Horizontal, 10., cx, div());
      assert!(anim.from.is_none());
      assert_eq!(anim.generation, 0);

      let _ = anim.animate("a", Axis::Vertical, 20., cx, div());
      assert_eq!(anim.from.map(|f| f.0), Some(10.));
      assert_eq!(anim.current_target, 20.);
      assert_eq!(anim.generation, 1);

      let _ = anim.animate("a", Axis::Vertical, 20., cx, div());
      assert_eq!(anim.generation, 1);
      let _ = anim.animate("a", Axis::Horizontal, 0., cx, div());
      assert_eq!(anim.generation, 2);
    });
  }

  #[gpui::test]
  fn duration_follows_the_config(cx: &mut TestAppContext) {
    cx.update(|cx| {
      let mut config = Config::default();
      config.shell.animation.speed = 2.;
      cx.set_global(config);
      let anim = SizeAnimation::new(Duration::from_secs(1));
      assert_eq!(anim.duration(cx), Duration::from_millis(500));
    });
  }
}
