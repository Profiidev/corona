use std::time::{Duration, Instant};

use gpui_kit::{
  Animation, AnimationExt, App, Entity, IntoElement, ParentElement, RenderOnce, StyleRefinement,
  Styled, Window, base::StyledExt, component::ActiveTheme, div, ease_out_quint, linear_color_stop,
  linear_gradient, prelude::FluentBuilder, px, relative,
};

use corona_config::ConfigProvider;

const TITLE_SIZE: f32 = 12.;
const TITLE_MAX_WIDTH: f32 = 100.;
const FADE_WIDTH: f32 = 16.;
const SCROLL_SPEED: f32 = 50.;
const SCROLL_GAP: f32 = 32.;
const SCROLL_RETURN: Duration = Duration::from_millis(250);

#[derive(IntoElement)]
pub struct ScrollingText {
  speed: f32,
  return_duration: Duration,
  gap: f32,
  max_width: f32,
  state: Entity<ScrollingTextState>,
  font_size: f32,
  content: String,
  fade_width: f32,
  style: StyleRefinement,
}

impl ScrollingText {
  pub fn width(&self, window: &Window) -> f32 {
    self.text_width(window).min(self.max_width)
  }

  fn offset(&self) -> f32 {
    self.fade_width / 2.5
  }

  fn text_width(&self, window: &Window) -> f32 {
    let font_size = px(self.font_size);
    let text_style = window.text_style();

    f32::from(
      window
        .text_system()
        .layout_line(
          &self.content,
          font_size,
          &[text_style.to_run(self.content.len())],
          None,
        )
        .width,
    ) + self.offset()
  }

  pub fn new(state: Entity<ScrollingTextState>) -> Self {
    ScrollingText {
      speed: SCROLL_SPEED,
      return_duration: SCROLL_RETURN,
      gap: SCROLL_GAP,
      max_width: TITLE_MAX_WIDTH,
      state,
      font_size: TITLE_SIZE,
      content: String::new(),
      fade_width: FADE_WIDTH,
      style: StyleRefinement::default(),
    }
  }

  pub fn content(mut self, content: impl Into<String>) -> Self {
    self.content = content.into();
    self
  }

  #[allow(unused)]
  pub fn font_size(mut self, font_size: f32) -> Self {
    self.font_size = font_size;
    self
  }

  #[allow(unused)]
  pub fn max_width(mut self, max_width: f32) -> Self {
    self.max_width = max_width;
    self
  }

  #[allow(unused)]
  pub fn fade_width(mut self, fade_width: f32) -> Self {
    self.fade_width = fade_width;
    self
  }

  #[allow(unused)]
  pub fn speed(mut self, speed: f32) -> Self {
    self.speed = speed;
    self
  }

  #[allow(unused)]
  pub fn return_duration(mut self, return_duration: Duration) -> Self {
    self.return_duration = return_duration;
    self
  }

  #[allow(unused)]
  pub fn gap(mut self, gap: f32) -> Self {
    self.gap = gap;
    self
  }
}

#[derive(Default)]
pub struct ScrollingTextState {
  return_from: Option<f32>,
  hovered: bool,
  hover_at: Option<Instant>,
  shift: f32,
  // the text's, set on render
  speed: f32,
  return_duration: Duration,
}

pub trait ScrollingTextExt {
  fn on_hover(&self) -> impl Fn(&bool, &mut Window, &mut App) + 'static;
  fn reset_hover(&self, cx: &mut App);
}

impl ScrollingTextExt for Entity<ScrollingTextState> {
  fn on_hover(&self) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
    let state = self.clone();
    move |hovered, _, cx| {
      state.update(cx, |this, cx| {
        this.hovered = *hovered;
        if *hovered {
          this.hover_at = Some(Instant::now());
          this.return_from = None;
        } else {
          let cycle = this.shift / this.speed;
          let elapsed = this
            .hover_at
            .take()
            .map_or(0., |t| t.elapsed().as_secs_f32());
          this.return_from = (cycle > 0.).then(|| (elapsed / cycle).fract());

          let ret = cx.config().shell.animation.duration(this.return_duration);

          cx.spawn(async move |this, cx| {
            cx.background_executor().timer(ret).await;
            this
              .update(cx, |this, cx| {
                this.return_from = None;
                cx.notify();
              })
              .ok();
          })
          .detach();
        }
        cx.notify();
      });
    }
  }

  fn reset_hover(&self, cx: &mut App) {
    self.update(cx, |s, _| {
      s.hovered = false;
      s.hover_at = None;
      s.return_from = None;
      s.shift = 0.;
    });
  }
}

impl Styled for ScrollingText {
  fn style(&mut self) -> &mut StyleRefinement {
    &mut self.style
  }
}

impl RenderOnce for ScrollingText {
  fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
    let font_size = px(self.font_size);
    let width = px(self.text_width(window));
    let shift = f32::from(width) + self.gap;

    self.state.update(cx, |s, _| {
      s.shift = shift;
      s.speed = self.speed;
      s.return_duration = self.return_duration;
    });

    let state = self.state.read(cx);
    let theme = cx.theme();
    let anim = cx.config().shell.animation.clone();

    let label = || {
      div()
        .flex_none()
        .whitespace_nowrap()
        .text_size(font_size)
        .line_height(relative(1.))
        .text_color(theme.tokens.secondary_foreground)
        .w(width)
        .child(self.content.clone())
    };

    let fade = theme.tokens.background.blend(*theme.tokens.button_hover);
    let fade_out = |angle: f32| {
      div()
        .absolute()
        .top_0()
        .bottom_0()
        .w(px(self.fade_width))
        .bg(linear_gradient(
          angle,
          linear_color_stop(fade.alpha(0.), 0.),
          linear_color_stop(fade, 1.),
        ))
    };

    let offset = self.offset();
    let id = self.state.entity_id();
    let overflowing = f32::from(width) > self.max_width;
    let scrolling = overflowing && (state.hovered || state.return_from.is_some());
    let track = div()
      .flex()
      .flex_none()
      .gap(px(self.gap))
      .ml(px(offset))
      .child(label());

    div()
      .relative()
      .flex_none()
      .overflow_hidden()
      .child(match (scrolling, state.return_from) {
        (true, None) => track
          .child(label())
          .with_animation(
            ("scrolling-text-scroll", id),
            Animation::new(Duration::from_secs_f32(shift / self.speed)).repeat(),
            move |this, delta| this.ml(px(offset - shift * delta)),
          )
          .into_any_element(),
        (true, Some(from)) => track
          .child(label())
          .with_animation(
            ("scrolling-text-return", id),
            Animation::new(anim.duration(self.return_duration)).with_easing(ease_out_quint()),
            move |this, delta| this.ml(px(offset - shift * from * (1. - delta))),
          )
          .into_any_element(),
        (false, _) => track.into_any_element(),
      })
      .child(fade_out(270.).left_neg_0p5())
      .when(overflowing, |this| {
        this.child(fade_out(90.).right_neg_0p5())
      })
      .refine_style(&self.style)
  }
}

#[cfg(test)]
mod tests {
  use gpui_kit::{self as gpui, AppContext, Entity, IntoElement, TestAppContext};

  use super::*;
  use crate::test_view;

  const LONG: &str = "a title far too long to fit in the hundred pixels it gets";

  fn setup(
    cx: &mut TestAppContext,
    text: impl Fn(Entity<ScrollingTextState>) -> ScrollingText + 'static,
  ) -> (gpui_kit::AnyWindowHandle, Entity<ScrollingTextState>) {
    let state = cx.new(|_| ScrollingTextState::default());
    let s = state.clone();
    let (handle, _) = test_view::open(cx, move |_, _| text(s.clone()).into_any_element());
    test_view::draw(handle, cx);
    (handle, state)
  }

  fn hover(
    handle: gpui_kit::AnyWindowHandle,
    state: &Entity<ScrollingTextState>,
    on: bool,
    cx: &mut TestAppContext,
  ) {
    let f = state.on_hover();
    cx.update_window(handle, |_, window, cx| f(&on, window, cx))
      .unwrap();
  }

  #[gpui::test]
  fn render_measures_the_shift(cx: &mut TestAppContext) {
    let (handle, state) = setup(cx, |s| ScrollingText::new(s).content(LONG));
    state.read_with(cx, |s, _| assert!(s.shift >= SCROLL_GAP));
    // never wider than its max
    let width = cx
      .update_window(handle, |_, window, _| {
        ScrollingText::new(state.clone())
          .content(LONG)
          .max_width(5.)
          .width(window)
      })
      .unwrap();
    assert!(width <= 5.);
  }

  #[gpui::test]
  fn hover_starts_and_leave_returns_then_settles(cx: &mut TestAppContext) {
    let (handle, state) = setup(cx, |s| ScrollingText::new(s).content(LONG));
    hover(handle, &state, true, cx);
    state.read_with(cx, |s, _| {
      assert!(s.hovered);
      assert!(s.hover_at.is_some());
      assert!(s.return_from.is_none());
    });
    test_view::draw(handle, cx);

    hover(handle, &state, false, cx);
    state.read_with(cx, |s, _| {
      assert!(!s.hovered);
      assert!(s.hover_at.is_none());
      let from = s.return_from.expect("returning");
      assert!((0. ..1.).contains(&from));
    });
    test_view::draw(handle, cx);
    cx.executor().advance_clock(SCROLL_RETURN);
    state.read_with(cx, |s, _| assert!(s.return_from.is_none()));
  }

  #[gpui::test]
  fn leave_without_shift_does_not_return(cx: &mut TestAppContext) {
    let (handle, state) = setup(cx, |s| ScrollingText::new(s).content("x"));
    state.update(cx, |s, _| s.shift = 0.);
    hover(handle, &state, false, cx);
    state.read_with(cx, |s, _| assert!(s.return_from.is_none()));
  }

  #[gpui::test]
  fn reset_hover_clears_everything(cx: &mut TestAppContext) {
    let (handle, state) = setup(cx, |s| ScrollingText::new(s).content(LONG));
    hover(handle, &state, true, cx);
    cx.update(|cx| state.reset_hover(cx));
    state.read_with(cx, |s, _| {
      assert!(!s.hovered && s.hover_at.is_none() && s.return_from.is_none());
      assert_eq!(s.shift, 0.);
    });
  }

  #[gpui::test]
  fn builders_render(cx: &mut TestAppContext) {
    let (handle, state) = setup(cx, |s| {
      ScrollingText::new(s)
        .content(LONG)
        .font_size(20.)
        .max_width(10.)
        .fade_width(4.)
        .speed(10.)
        .gap(8.)
        .return_duration(Duration::from_millis(10))
    });
    hover(handle, &state, true, cx);
    test_view::draw(handle, cx);
    hover(handle, &state, false, cx);
    test_view::draw(handle, cx);
    state.read_with(cx, |s, _| assert!(s.shift >= 8.));
  }

  #[gpui::test]
  fn return_uses_custom_speed(cx: &mut TestAppContext) {
    let speed = 100.;
    let (handle, state) = setup(cx, move |s| {
      ScrollingText::new(s).content(LONG).speed(speed)
    });
    hover(handle, &state, true, cx);
    // hovered for one and a half scroll cycles at this text's speed
    state.update(cx, |s, _| {
      let cycle = Duration::from_secs_f32(s.shift / speed);
      s.hover_at = Instant::now().checked_sub(cycle.mul_f32(1.5));
    });
    hover(handle, &state, false, cx);
    let from = state.read_with(cx, |s, _| s.return_from.unwrap());
    assert!((from - 0.5).abs() < 0.05, "{from}");
  }
}
