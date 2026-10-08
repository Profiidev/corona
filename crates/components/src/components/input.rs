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

#[cfg(test)]
mod tests {
  use gpui_kit::{self as gpui, AppContext, IntoElement, TestAppContext, div};

  use super::*;
  use crate::test_view;

  struct Form {
    submitted: usize,
    _sub: Option<Subscription>,
  }

  #[gpui::test]
  fn only_enter_submits(cx: &mut TestAppContext) {
    let (handle, _) = test_view::open(cx, |_, _| div().into_any_element());
    let (form, input) = cx
      .update_window(handle, |_, window, cx| {
        let input = cx.new(|cx| InputState::new(window, cx));
        let form = cx.new(|cx| {
          let sub = on_enter(&input, window, cx, |this: &mut Form, _, _| {
            this.submitted += 1
          });
          Form {
            submitted: 0,
            _sub: Some(sub),
          }
        });
        (form, input)
      })
      .unwrap();
    for event in [
      InputEvent::Change,
      InputEvent::PressEnter {
        secondary: false,
        shift: false,
      },
      InputEvent::Focus,
      InputEvent::PressEnter {
        secondary: true,
        shift: true,
      },
    ] {
      input.update(cx, |_, cx| cx.emit(event));
    }
    cx.run_until_parked();
    form.read_with(cx, |f, _| assert_eq!(f.submitted, 2));
  }
}
