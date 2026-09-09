use std::time::{Duration, Instant};

use gpui_kit::{
  Animation, AnimationExt, AnyElement, App, Axis, IntoElement, Styled, ease_out_quint, px,
};

use crate::config::ConfigProvider;

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
    self.duration.mul_f32(cx.config().animation_speed)
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
