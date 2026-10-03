use std::{cell::Cell, rc::Rc, time::Duration};

use corona_components::animation::bounds::BoundsAnimation;
use gpui_kit::{
  Bounds, Context, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels, Styled,
  Window,
  base::{ElementExt, h_flex},
  component::{
    ActiveTheme, ThemeStyled,
    button::{Button, ButtonCustomVariant, ButtonVariant, ButtonVariants},
  },
  div, point, px,
};

use crate::overlays::screenshot::{
  mode::Mode, overlay::Overlay, save::is_empty, state::ScreenshotState,
};

pub struct ScreenshotToolbar {
  toolbar_bounds: Rc<Cell<Bounds<Pixels>>>,
  mode_bounds: [Rc<Cell<Bounds<Pixels>>>; 3],
  pill: BoundsAnimation,
}

impl ScreenshotToolbar {
  pub fn new() -> Self {
    Self {
      toolbar_bounds: Default::default(),
      mode_bounds: Default::default(),
      pill: BoundsAnimation::default(),
    }
  }

  pub fn render(
    &mut self,
    mode: Mode,
    duration: Duration,
    window: &mut Window,
    cx: &mut Context<Overlay>,
  ) -> impl IntoElement {
    let active = Mode::ALL.iter().position(|m| *m == mode).unwrap_or(0);
    let button = self.mode_bounds[active].get();
    let target = (!is_empty(&button)).then(|| Bounds {
      origin: point(px(0.), px(0.)) + (button.origin - self.toolbar_bounds.get().origin),
      size: button.size,
    });
    let (pill, moving) = self.pill.step(target, duration);
    if moving || target.is_none() {
      window.request_animation_frame();
    }

    let theme = cx.theme();
    let (pill_bg, on_pill) = (theme.tokens.button_primary, theme.button_primary_foreground);
    let clear = gpui_kit::transparent_black();
    let on_pill_variant = ButtonCustomVariant::new(cx)
      .color(clear)
      .hover(clear)
      .active(clear)
      .foreground(on_pill);

    h_flex()
      .absolute()
      .bottom(px(24.))
      .left_0()
      .right_0()
      .justify_center()
      .child(
        h_flex()
          .relative()
          .popover_style(cx)
          .rounded_full()
          .p_1()
          .gap_1()
          .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
          .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
          .child(div().absolute().inset_0().on_prepaint({
            let bounds = self.toolbar_bounds.clone();
            move |b, _, _| bounds.set(b)
          }))
          .children(pill.map(|b| {
            div()
              .absolute()
              .left(b.origin.x)
              .top(b.origin.y)
              .w(b.size.width)
              .h(b.size.height)
              .rounded_full()
              .bg(pill_bg)
          }))
          .children(Mode::ALL.into_iter().enumerate().map(|(i, m)| {
            let bounds = self.mode_bounds[i].clone();
            div().on_prepaint(move |b, _, _| bounds.set(b)).child(
              Button::new(m.label())
                .with_variant(if m == mode {
                  ButtonVariant::Custom(on_pill_variant)
                } else {
                  ButtonVariant::Ghost
                })
                .rounded_full()
                .cursor_pointer()
                .icon(m.icon())
                .label(m.label())
                .tooltip(m.hint())
                .on_click(cx.listener(move |_, _, _, cx| {
                  ScreenshotState::set_mode(m, cx);
                  cx.notify();
                })),
            )
          })),
      )
  }
}
