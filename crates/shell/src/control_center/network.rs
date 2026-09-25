use gpui_kit::{Context, IntoElement, Render, Window};

use crate::control_center::ControlCenterPanel;

pub struct NetworkPanel {}

impl ControlCenterPanel for NetworkPanel {
  fn init(_window: &mut Window, _cx: &mut Context<'_, Self>) -> Self {
    Self {}
  }
}

impl Render for NetworkPanel {
  fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
    "network"
  }
}
