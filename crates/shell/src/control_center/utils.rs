use std::f32::consts::TAU;

use gpui_kit::{
  App, Bounds, Context, Div, Entity, Hsla, InteractiveElement, IntoElement, MouseButton,
  MouseDownEvent, ParentElement, Styled, Subscription, Window,
  base::input::{InputEvent, InputState},
  canvas,
  component::{
    Theme,
    plot::shape::{Arc, ArcData},
  },
  div, fill, point, px, size,
};

pub fn on_enter<T: 'static>(
  input: &Entity<InputState>,
  window: &mut Window,
  cx: &mut Context<T>,
  submit: fn(&mut T, &mut Window, &mut Context<T>),
) -> Subscription {
  cx.subscribe_in(
    input,
    window,
    move |this, _, event: &InputEvent, window, cx| {
      if let InputEvent::PressEnter { .. } = event {
        submit(this, window, cx);
      }
    },
  )
}

pub fn overlay(
  theme: &Theme,
  card: Div,
  on_close: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> Div {
  div()
    .on_mouse_down(MouseButton::Left, on_close)
    .absolute()
    .inset_0()
    .flex()
    .items_center()
    .justify_center()
    .rounded_xl()
    .bg(theme.colors.background.opacity(0.8))
    .occlude()
    .child(
      card
        .flex()
        .flex_col()
        .w_3_4()
        .gap_2()
        .p_4()
        .rounded_xl()
        .border_1()
        .border_color(theme.colors.border)
        .bg(theme.colors.background)
        // clicks inside the card must not reach the background
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
    )
}

pub fn ring(progress: f32, color: Hsla, width: f32) -> impl IntoElement {
  canvas(
    |_, _, _| {},
    move |bounds, _, window, _| {
      let radius = (bounds.size.width.min(bounds.size.height).as_f32() - width) / 2.;
      let arc = Arc::new()
        .inner_radius(radius - width / 2.)
        .outer_radius(radius + width / 2.);
      arc.paint(
        &ArcData::new(&(), 0, 1., 0., TAU),
        color.opacity(0.2),
        &bounds,
        window,
      );
      if progress <= 0. {
        return;
      }
      let end = progress * TAU;
      arc.paint(
        &ArcData::new(&(), 1, progress, 0., end),
        color,
        &bounds,
        window,
      );

      let center = bounds.center();
      for angle in [0., end] {
        let dot = point(
          center.x + px(radius * angle.sin() - width / 2.),
          center.y - px(radius * angle.cos() + width / 2.),
        );
        window.paint_quad(
          fill(Bounds::new(dot, size(px(width), px(width))), color).corner_radii(px(width / 2.)),
        );
      }
    },
  )
  .absolute()
  .size_full()
}
