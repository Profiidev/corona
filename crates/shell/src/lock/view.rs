use std::{
  cell::Cell,
  rc::Rc,
  sync::Arc,
  time::{Duration, Instant},
};

use corona_auth::AuthExt;
use corona_auth_screen::{AuthScreen, Purpose};
use corona_components::animation::animation_duration;
use gpui_kit::{
  App, AppContext, BoxShadow, Context, Div, Entity, IntoElement, ParentElement, Pixels, Render,
  RenderImage, Size, Styled, Window, black, component::ActiveTheme, div, ease_out_quint, img,
  prelude::FluentBuilder, px, relative,
};

use crate::lock::{state::LockState, user};

const ZOOM: f32 = 0.96;
const FEATHER: f32 = 10.;
/// Darkens the blurred screen under the lock UI
const DIM: f32 = 0.5;
pub(super) const ZOOM_SPEED: Duration = Duration::from_millis(500);

#[derive(Clone)]
pub struct Background {
  pub sharp: Arc<RenderImage>,
  pub blurred: Arc<RenderImage>,
}

pub struct Lock {
  pub screen: Entity<AuthScreen>,
  background: Option<Background>,
}

impl Lock {
  pub fn new(background: Option<Background>, window: &mut Window, cx: &mut Context<Self>) -> Self {
    let user = user(cx);
    let check = Rc::new(|user: String, password: String, cx: &mut App| {
      let check = cx.auth().password(user, password, cx);
      cx.spawn(async move |cx| {
        let right = check.await?;
        if right {
          cx.update(LockState::unlock_animated);
        }
        Ok(right)
      })
    });
    Self {
      screen: cx.new(|cx| AuthScreen::new(Purpose::Unlock, user, check, window, cx)),
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
    .child(
      div()
        .absolute()
        .size_full()
        .bg(black().opacity(DIM * progress)),
    )
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
    div()
      .size_full()
      .relative()
      .bg(theme.background)
      .text_color(theme.foreground)
      .when_some(self.background.clone(), |d, background| {
        d.child(screen(background, progress, size))
      })
      .child(
        div()
          .absolute()
          .size_full()
          .opacity(progress)
          .child(self.screen.clone()),
      )
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
    div()
      .size_full()
      .bg(theme.background)
      .opacity(progress)
      .when_some(self.background.clone(), |d, background| {
        d.child(screen(background, progress, size))
      })
  }
}
