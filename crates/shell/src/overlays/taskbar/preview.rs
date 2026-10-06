use std::time::Duration;

use corona_capture::LiveCapture;
use corona_components::animation::{animation_duration, glide::Glide};
use corona_compositor::types;
use corona_config::ConfigProvider;
use gpui_kit::{
  App, AppContext, Bounds, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render,
  StatefulInteractiveElement, Styled, WeakEntity, Window, component::ActiveTheme, div, point,
  prelude::FluentBuilder, px, size,
};

use corona_surface::input_region::InputRegion;

use super::{Taskbar, focus_window};

const FPS: u32 = 10;
const HEIGHT: f32 = 135.;
const MIN_WIDTH: f32 = 80.;
const MAX_WIDTH: f32 = 320.;
const TITLE: f32 = 20.;
const GAP: f32 = 8.;
const PADDING: f32 = 8.;
const BORDER: f32 = 2.;
const MORE_WIDTH: f32 = 48.;
const SLIDE_SPEED: Duration = Duration::from_millis(100);

pub const POPUP_HEIGHT: f32 = HEIGHT + 4. + TITLE + (PADDING + BORDER) * 2.;

fn width(window: &types::Window) -> f32 {
  let aspect = window.width.max(1) as f32 / window.height.max(1) as f32;
  (HEIGHT * aspect).clamp(MIN_WIDTH, MAX_WIDTH)
}

pub struct Preview {
  taskbar: WeakEntity<Taskbar>,
  windows: Vec<(types::Window, Entity<LiveCapture>)>,
  more: usize,
  center: f32,
  slide: Glide<1>,
  hovered: Option<usize>,
  highlight: Glide<2>,
  input_region: InputRegion,
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
      more: windows.len().saturating_sub(max_windows(cx)),
      windows: live(windows, cx),
      center,
      slide: Glide::default(),
      hovered: None,
      highlight: Glide::default(),
      input_region: InputRegion::default(),
    }
  }

  pub fn show(&mut self, windows: Vec<types::Window>, center: f32, cx: &mut Context<Self>) {
    self.more = windows.len().saturating_sub(max_windows(cx));
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

fn max_windows(cx: &App) -> usize {
  cx.config().taskbar.preview_max_windows
}

fn live(windows: Vec<types::Window>, cx: &mut App) -> Vec<(types::Window, Entity<LiveCapture>)> {
  windows
    .into_iter()
    .take(max_windows(cx))
    .map(|w| {
      let live = cx.new(|cx| LiveCapture::window(&w.address, FPS, cx).rounded(cx.theme().radius));
      (w, live)
    })
    .collect()
}

impl Render for Preview {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let viewport = window.viewport_size().width.as_f32();
    let width = self.card_width();
    let target = (self.center - width / 2.).clamp(0., (viewport - width).max(0.));
    let speed = animation_duration(SLIDE_SPEED, cx);
    let ([x], sliding) = self.slide.value([target], speed);
    let highlight = match self.hovered.filter(|&i| i < self.windows.len()) {
      Some(i) => {
        let tile = self.tile(i);
        Some(self.highlight.value(tile, speed))
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
    self.input_region.set(region, window);

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
