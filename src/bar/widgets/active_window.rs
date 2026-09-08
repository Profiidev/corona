use std::time::{Duration, Instant};

use gpui_kit::{
  Animation, AnimationExt, AnyElement, Axis, Context, Empty, InteractiveElement, IntoElement,
  ParentElement, Render, StatefulInteractiveElement, Styled, Subscription, Window,
  component::ActiveTheme, div, ease_out_quint, img, linear_color_stop, linear_gradient,
  prelude::FluentBuilder, px, relative,
};
use uuid::Uuid;

use crate::{
  bar::{anim::SizeAnimation, style::BarStyle, widgets::Widget},
  compositor::{CompositorExt, event::CompositorEvent, types},
  config::ConfigProvider,
  desktop_entry::icon_for_class_or_default,
  error::ErrorLogExt,
};

const ICON_SIZE: u16 = 18;
const TITLE_SIZE: f32 = 12.;
const TITLE_MAX_WIDTH: f32 = 100.;
const FADE_WIDTH: f32 = 16.;
/// pixels per second.
const SCROLL_SPEED: f32 = 60.;
const SCROLL_GAP: f32 = 32.;
const SCROLL_RETURN: Duration = Duration::from_millis(250);
const WIDTH_CHANGE: Duration = Duration::from_millis(400);

pub struct ActiveWindow {
  active: Option<types::Window>,
  hovered: bool,
  /// When the current hover started
  hover_at: Option<Instant>,
  /// Marquee progress (0..1) the pointer left at, while easing back to 0.
  return_from: Option<f32>,
  /// Distance of one marquee cycle, measured during the last render.
  shift: f32,
  /// Eases the pill between widths when the title length changes.
  size: SizeAnimation,
  #[allow(dead_code)]
  subscription: Subscription,
}

impl Widget for ActiveWindow {
  fn init(cx: &mut Context<'_, Self>, _display_id: Uuid) -> Self {
    let compositor = cx.compositor();
    let active = compositor.active_window().log_err().ok().flatten();

    let emitter = compositor.emitter().clone();
    let subscription = cx.subscribe(&emitter, move |this, _, e, cx| {
      if let CompositorEvent::ActiveWindow(window) = e {
        this.active = window.clone();
        cx.notify();
      }
    });

    ActiveWindow {
      active,
      hovered: false,
      hover_at: None,
      return_from: None,
      shift: 0.,
      size: SizeAnimation::new(WIDTH_CHANGE),
      subscription,
    }
  }
}

impl ActiveWindow {
  fn render_title(&mut self, title: &str, window: &Window, cx: &mut Context<Self>) -> AnyElement {
    let theme = cx.theme();
    let config = cx.config();
    let anim = config.animation_speed;

    let font_size = px(TITLE_SIZE);
    let text_style = window.text_style();
    let width = window
      .text_system()
      .layout_line(title, font_size, &[text_style.to_run(title.len())], None)
      .width;

    let label = || {
      div()
        .flex_none()
        .whitespace_nowrap()
        .text_size(font_size)
        .line_height(relative(1.))
        .text_color(theme.tokens.secondary_foreground)
        .w(width)
        .child(title.to_string())
    };

    let target = f32::from(width).min(TITLE_MAX_WIDTH);

    let fade = theme.tokens.background.blend(*theme.tokens.button_hover);
    let fade_out = |angle: f32| {
      div()
        .absolute()
        .top_0()
        .bottom_0()
        .w(px(FADE_WIDTH))
        .bg(linear_gradient(
          angle,
          linear_color_stop(fade.alpha(0.), 0.),
          linear_color_stop(fade, 1.),
        ))
    };

    let shift = f32::from(width) + SCROLL_GAP;
    self.shift = shift;
    let overflowing = f32::from(width) > TITLE_MAX_WIDTH;
    let scrolling = overflowing && (self.hovered || self.return_from.is_some());
    let track = div().flex().flex_none().gap(px(SCROLL_GAP)).child(label());

    let title = div()
      .relative()
      .flex_none()
      .overflow_hidden()
      .child(match (scrolling, self.return_from) {
        (true, None) => track
          .child(label())
          .with_animation(
            "active-window-scroll",
            Animation::new(Duration::from_secs_f32(shift / SCROLL_SPEED)).repeat(),
            move |this, delta| this.ml(px(-shift * delta)),
          )
          .into_any_element(),
        (true, Some(from)) => track
          .child(label())
          .with_animation(
            "active-window-return",
            Animation::new(SCROLL_RETURN.mul_f32(anim)).with_easing(ease_out_quint()),
            move |this, delta| this.ml(px(-shift * from * (1. - delta))),
          )
          .into_any_element(),
        (false, _) => track.into_any_element(),
      })
      .when(scrolling, |this| this.child(fade_out(270.).left_0()))
      .when(overflowing || self.size.animating(cx), |this| {
        this.child(fade_out(90.).right_0())
      });

    self
      .size
      .animate("active-window-title", Axis::Horizontal, target, cx, title)
  }
}

impl Render for ActiveWindow {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let Some(active_window) = self.active.clone() else {
      return Empty.into_any_element();
    };

    let theme = cx.theme();
    let config = cx.config();
    let anim = config.animation_speed;
    let icon = icon_for_class_or_default(&active_window.class, ICON_SIZE);

    div()
      .id("active-window")
      .flex_bar(window, cx)
      .items_center()
      .justify_center()
      .gap_1()
      .px_2()
      .h(px(24.))
      .min_w(px(36.))
      .rounded_full()
      .bg(theme.tokens.button_hover)
      .on_hover(cx.listener(move |this, hovered, _, cx| {
        this.hovered = *hovered;
        if *hovered {
          this.hover_at = Some(Instant::now());
          this.return_from = None;
        } else {
          let cycle = this.shift / SCROLL_SPEED;
          let elapsed = this
            .hover_at
            .take()
            .map_or(0., |t| t.elapsed().as_secs_f32());
          this.return_from = (cycle > 0.).then(|| (elapsed / cycle).fract());
          cx.spawn(async move |this, cx| {
            cx.background_executor()
              .timer(SCROLL_RETURN.mul_f32(anim))
              .await;
            this
              .update(cx, |this: &mut Self, cx| {
                this.return_from = None;
                cx.notify();
              })
              .ok();
          })
          .detach();
        }
        cx.notify();
      }))
      .child(
        div()
          .flex()
          .items_center()
          .justify_center()
          .relative()
          .h(px(ICON_SIZE as f32))
          .w(px(ICON_SIZE as f32))
          .rounded_full()
          .map(|this| match icon {
            Some(path) => this.child(img(path).size_full()),
            None => this.text_size(px(10.)).line_height(relative(1.)).child(
              active_window
                .class
                .chars()
                .next()
                .unwrap_or('?')
                .to_string(),
            ),
          }),
      )
      .child(self.render_title(&active_window.title, window, cx))
      .into_any_element()
  }
}
