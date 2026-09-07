use gpui_kit::{
  Context, IntoElement, Render, Styled, Window,
  base::FocusableExt,
  component::{self, Sizable, button::ButtonVariants},
  px,
};

use crate::{
  assets::icons::IconName,
  bar::{toggle_panel, widgets::Widget},
  panel::ControlPanel,
};

pub struct Button;

impl Widget for Button {}

impl Render for Button {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    component::button::Button::new("test-button")
      .secondary()
      .rounded_full()
      .focus_ring(false)
      .with_size(px(24.))
      .cursor_pointer()
      .icon(IconName::Nixos)
      .on_click(cx.listener(|_, _, window, cx| {
        toggle_panel(ControlPanel, window, cx).expect("Failed to toggle control panel");
      }))
  }
}
