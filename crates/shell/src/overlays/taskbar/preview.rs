use std::time::Duration;

use corona_capture::LiveCapture;
use corona_components::animation::smooth_retarget::SmoothRetarget;
use corona_compositor::types;
use corona_config::ConfigProvider;
use gpui_kit::{
  App, AppContext, Bounds, Context, Entity, InteractiveElement, IntoElement, ParentElement, Pixels,
  Render, StatefulInteractiveElement, Styled, WeakEntity, Window, component::ActiveTheme, div,
  point, prelude::FluentBuilder, px, size,
};

use super::{Taskbar, focus_window};

const FPS: u32 = 10;
const HEIGHT: f32 = 135.;
const MIN_WIDTH: f32 = 80.;
const MAX_WIDTH: f32 = 320.;
const TITLE: f32 = 20.;
const GAP: f32 = 8.;
const PADDING: f32 = 8.;
const BORDER: f32 = 2.;
const MAX_WINDOWS: usize = 5;
const MORE_WIDTH: f32 = 48.;
const SLIDE_SPEED: Duration = Duration::from_millis(100);

pub const POPUP_HEIGHT: f32 = HEIGHT + 4. + TITLE + (PADDING + BORDER) * 2.;

fn width(window: &types::Window) -> f32 {
  let aspect = window.width.max(1) as f32 / window.height.max(1) as f32;
  (HEIGHT * aspect).clamp(MIN_WIDTH, MAX_WIDTH)
}

struct Glide<const N: usize> {
  from: [f32; N],
  to: Option<[f32; N]>,
  anim: SmoothRetarget,
}

impl<const N: usize> Glide<N> {
  fn new() -> Self {
    Self {
      from: [0.; N],
      to: None,
      anim: SmoothRetarget::new(1.),
    }
  }

  fn reset(&mut self) {
    self.to = None;
  }

  fn value(&mut self, target: [f32; N], cx: &App) -> ([f32; N], bool) {
    let Some(to) = self.to else {
      self.to = Some(target);
      return (target, false);
    };
    let (progress, moving) = self.anim.value();
    let current = std::array::from_fn(|i| self.from[i] + (to[i] - self.from[i]) * progress);
    if to == target {
      return (current, moving);
    }
    let speed = if cx.reduce_motion() {
      Duration::ZERO
    } else {
      SLIDE_SPEED.mul_f32(cx.config().animation_speed)
    };
    self.from = current;
    self.to = Some(target);
    self.anim = SmoothRetarget::new(0.);
    self.anim.retarget(1., speed);
    (current, true)
  }
}

pub struct Preview {
  taskbar: WeakEntity<Taskbar>,
  windows: Vec<(types::Window, Entity<LiveCapture>)>,
  more: usize,
  center: f32,
  slide: Glide<1>,
  hovered: Option<usize>,
  highlight: Glide<2>,
  input_region: Option<Bounds<Pixels>>,
}

impl Preview {
  pub fn new(
    taskbar: WeakEntity<Taskbar>,
    windows: Vec<types::Window>,
    center: f32,
    cx: &mut App,
  ) -> Self {
    Self {
      taskbar,
      more: windows.len().saturating_sub(MAX_WINDOWS),
      windows: live(windows, cx),
      center,
      slide: Glide::new(),
      hovered: None,
      highlight: Glide::new(),
      input_region: None,
    }
  }

  pub fn show(&mut self, windows: Vec<types::Window>, center: f32, cx: &mut Context<Self>) {
    self.more = windows.len().saturating_sub(MAX_WINDOWS);
    self.windows = live(windows, cx);
    self.center = center;
    self.hovered = None;
    self.highlight.reset();
    cx.notify();
  }

  fn card_width(&self) -> f32 {
    let more = if self.more > 0 { MORE_WIDTH } else { 0. };
    let widths: f32 = self.windows.iter().map(|(w, _)| width(w)).sum::<f32>() + more;
    let tiles = self.windows.len() + usize::from(self.more > 0);
    let gaps = GAP * tiles.saturating_sub(1) as f32;
    widths + gaps + (PADDING + BORDER) * 2.
  }

  fn tile(&self, index: usize) -> [f32; 2] {
    let left: f32 = self.windows[..index]
      .iter()
      .map(|(w, _)| width(w) + GAP)
      .sum();
    [PADDING + left, width(&self.windows[index].0)]
  }
}

fn live(windows: Vec<types::Window>, cx: &mut App) -> Vec<(types::Window, Entity<LiveCapture>)> {
  windows
    .into_iter()
    .take(MAX_WINDOWS)
    .map(|w| {
      let live = cx.new(|cx| LiveCapture::window(&w.address, FPS, cx));
      (w, live)
    })
    .collect()
}

impl Render for Preview {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let viewport = window.viewport_size().width.as_f32();
    let width = self.card_width();
    let target = (self.center - width / 2.).clamp(0., (viewport - width).max(0.));
    let ([x], sliding) = self.slide.value([target], cx);
    let highlight = match self.hovered.filter(|&i| i < self.windows.len()) {
      Some(i) => {
        let tile = self.tile(i);
        Some(self.highlight.value(tile, cx))
      }
      None => None,
    };
    if sliding || highlight.is_some_and(|(_, moving)| moving) {
      window.request_animation_frame();
    }

    let region = Bounds::new(
      point(px(x.round()), px(0.)),
      size(px(width), px(POPUP_HEIGHT)),
    );
    if self.input_region != Some(region) {
      self.input_region = Some(region);
      window.set_input_region(Some(&[region]));
    }

    let theme = cx.theme();
    let taskbar = self.taskbar.clone();
    div().size_full().relative().child(
      div()
        .id("taskbar-preview")
        .absolute()
        .top_0()
        .left(px(x.round()))
        .w(px(width))
        .h(px(POPUP_HEIGHT))
        .flex()
        .gap(px(GAP))
        .p(px(PADDING))
        .bg(theme.tokens.background)
        .rounded(theme.radius)
        .border(px(BORDER))
        .border_color(theme.tokens.button_hover)
        .text_color(theme.foreground)
        .on_hover(move |hovered, _, cx| {
          let hovered = *hovered;
          let _ = taskbar.update(cx, |taskbar, cx| taskbar.preview_hovered(hovered, cx));
        })
        .children(self.windows.iter().enumerate().map(|(i, (w, live))| {
          let address = w.address.clone();
          let taskbar = self.taskbar.clone();
          div()
            .id(("preview", i))
            .on_hover(cx.listener(move |this, hovered, _, cx| {
              if *hovered {
                this.hovered = Some(i);
              } else if this.hovered == Some(i) {
                this.hovered = None;
              }
              cx.notify();
            }))
            .flex()
            .flex_col()
            .gap_1()
            .w(px(self::width(w)))
            .cursor_pointer()
            .on_click(move |_, _, cx| {
              focus_window(&address, cx);
              let _ = taskbar.update(cx, |taskbar, cx| taskbar.hide_preview(cx));
            })
            .child(
              div()
                .h(px(HEIGHT))
                .w_full()
                .rounded(theme.radius)
                .overflow_hidden()
                .bg(theme.tokens.button_hover)
                .child(live.clone()),
            )
            .child(
              div()
                .h(px(TITLE))
                .text_xs()
                .truncate()
                .child(w.title.clone()),
            )
        }))
        .when_some(highlight, |d, ([left, width], _)| {
          let outset = GAP / 2.;
          d.child(
            div()
              .absolute()
              .top(px(PADDING - outset))
              .left(px(left - outset))
              .w(px(width + outset * 2.))
              .h(px(HEIGHT + 4. + TITLE + outset * 2.))
              .rounded(theme.radius)
              .border_2()
              .border_color(theme.colors.primary),
          )
        })
        .when(self.more > 0, |d| {
          d.child(
            div()
              .w(px(MORE_WIDTH))
              .h(px(HEIGHT))
              .flex()
              .items_center()
              .justify_center()
              .text_sm()
              .text_color(theme.muted_foreground)
              .child(format!("+{}", self.more)),
          )
        }),
    )
  }
}
