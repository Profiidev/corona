use gpui_kit::{Context, IntoElement, Render, Window};

use crate::ui::app::control_center::ControlCenterPanel;

pub struct AudioPanel {}

impl ControlCenterPanel for AudioPanel {}

impl Render for AudioPanel {
  fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
    "audio"
  }
}
