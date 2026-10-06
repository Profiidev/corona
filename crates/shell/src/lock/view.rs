use std::sync::Arc;

use gpui_kit::{
  Context, FocusHandle, InteractiveElement, IntoElement, ParentElement, Render, RenderImage,
  Styled, Window, component::ActiveTheme, div, img, prelude::FluentBuilder,
};

use crate::lock::state::LockState;

pub struct Lock {
  pub focus: FocusHandle,
  background: Option<Arc<RenderImage>>,
}

impl Lock {
  pub fn new(background: Option<Arc<RenderImage>>, cx: &mut Context<Self>) -> Self {
    Self {
      focus: cx.focus_handle(),
      background,
    }
  }
}

impl Render for Lock {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();

    div()
      .track_focus(&self.focus)
      .on_key_down(cx.listener(|_, event: &gpui_kit::KeyDownEvent, _, cx| {
        if event.keystroke.key == "escape" {
          LockState::unlock(cx);
        }
      }))
      .size_full()
      .flex()
      .items_center()
      .justify_center()
      .bg(theme.background)
      .text_color(theme.foreground)
      .when_some(self.background.clone(), |d, image| {
        d.child(img(image).absolute().size_full())
      })
      .child("Locked — press Escape")
  }
}
