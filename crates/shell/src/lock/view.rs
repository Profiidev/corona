use std::{
  cell::Cell,
  rc::Rc,
  sync::Arc,
  time::{Duration, Instant},
};

use corona_components::animation::animation_duration;
use gpui_kit::{
  App, BoxShadow, Context, Div, FocusHandle, InteractiveElement, IntoElement, ParentElement,
  Pixels, Render, RenderImage, Size, Styled, Window, black, component::ActiveTheme, div,
  ease_out_quint, img, prelude::FluentBuilder, px, relative,
};

use crate::lock::state::LockState;
use rust_i18n::t;

const ZOOM: f32 = 0.96;
const FEATHER: f32 = 10.;
pub(super) const ZOOM_SPEED: Duration = Duration::from_millis(500);

#[derive(Clone)]
pub struct Background {
  pub sharp: Arc<RenderImage>,
  pub blurred: Arc<RenderImage>,
}

pub struct Lock {
  pub focus: FocusHandle,
  background: Option<Background>,
}

impl Lock {
  pub fn new(background: Option<Background>, cx: &mut Context<Self>) -> Self {
    Self {
      focus: cx.focus_handle(),
      background,
    }
  }
}

/// The screen `progress` of the way from as it was (0) to locked (1): smaller,
/// blurred, and fading to black towards the screen edge
fn screen(background: Background, progress: f32, size: Size<Pixels>) -> Div {
  let scale = 1. - (1. - ZOOM) * progress;
  let inset = relative((1. - scale) / 2.);
  let image = |image: Arc<RenderImage>| img(image).absolute().size_full();
  // a gaussian from the screen edge across the gap and a bit into the zoomed
  // screen, hiding where it meets the blur behind
  let feather = size.width.max(size.height) * ((1. - scale) / 2. * FEATHER);
  let spread = feather / 2.;
  let round = feather;
  // reaches past the screen so its own rounded corners stay off screen
  let overhang = spread + round;
  let vignette = div()
    .absolute()
    .top(-overhang)
    .left(-overhang)
    .w(size.width + overhang * 2.)
    .h(size.height + overhang * 2.)
    .rounded(overhang + round)
    .shadow(vec![
      BoxShadow::new(px(0.), px(0.), black())
        .blur_radius(feather / 4.)
        .spread_radius(overhang + spread)
        .inset(),
    ]);
  div()
    .absolute()
    .size_full()
    .bg(black())
    .child(image(background.blurred.clone()))
    .child(
      div()
        .absolute()
        .left(inset)
        .top(inset)
        .w(relative(scale))
        .h(relative(scale))
        .child(image(background.blurred))
        .child(image(background.sharp).opacity(1. - progress)),
    )
    .child(vignette)
}

/// How far an animation started at `start` is, eased. Keeps the frames coming
/// until it is done
fn eased(start: Instant, window: &mut Window, cx: &App) -> f32 {
  let duration = animation_duration(ZOOM_SPEED, cx);
  let delta = match duration {
    Duration::ZERO => 1.,
    _ => (start.elapsed().as_secs_f32() / duration.as_secs_f32()).min(1.),
  };
  if delta < 1. {
    window.request_animation_frame();
  }
  ease_out_quint()(delta)
}

impl Render for Lock {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let progress = eased(LockState::animation_start(cx), window, cx);

    let theme = cx.theme();
    let size = window.viewport_size();
    let label = div().relative().child(t!("app.lock.locked"));
    div()
      .track_focus(&self.focus)
      .on_key_down(cx.listener(|_, event: &gpui_kit::KeyDownEvent, _, cx| {
        if event.keystroke.key == "escape" {
          LockState::unlock_animated(cx);
        }
      }))
      .size_full()
      .flex()
      .items_center()
      .justify_center()
      .bg(theme.background)
      .text_color(theme.foreground)
      .when_some(self.background.clone(), |d, background| {
        d.child(screen(background, progress, size))
      })
      .child(label.opacity(progress))
  }
}

/// The lock animation played backwards on a click-through overlay, fading out
/// to the live desktop. Sits under the lock showing it, so taking the lock away
/// changes nothing on screen until the animation runs
pub struct Unlock {
  background: Option<Background>,
  /// Shared by every display so they play in sync
  start: Rc<Cell<Option<Instant>>>,
}

impl Unlock {
  pub fn new(background: Option<Background>, start: Rc<Cell<Option<Instant>>>) -> Self {
    Self { background, start }
  }
}

impl Render for Unlock {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let progress = match self.start.get() {
      Some(start) => 1. - eased(start, window, cx),
      None => 1.,
    };

    let theme = cx.theme();
    let size = window.viewport_size();
    let label = div().relative().child(t!("app.lock.locked"));
    div()
      .size_full()
      .flex()
      .items_center()
      .justify_center()
      .bg(theme.background)
      .text_color(theme.foreground)
      .opacity(progress)
      .when_some(self.background.clone(), |d, background| {
        d.child(screen(background, progress, size))
      })
      .child(label.opacity(progress))
  }
}
