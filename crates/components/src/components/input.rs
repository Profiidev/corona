use gpui_kit::{
  Context, Entity, Subscription, Window,
  base::input::{InputEvent, InputState},
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
