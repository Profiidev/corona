use gpui_kit::{
  App, Context, Div, Entity, InteractiveElement, MouseButton, MouseDownEvent, ParentElement,
  Styled, Subscription, Window,
  base::input::{InputEvent, InputState},
  component::Theme,
  div,
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
