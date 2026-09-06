use gpui_kit::{
  Context, IntoElement, Render, Window,
  component::{self, IconName},
};

use crate::bar::widgets::Widget;

pub struct Button;

impl Widget for Button {}

impl Render for Button {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    component::button::Button::new("test-button").icon(IconName::ArrowLeft)
  }
}
