use gpui_kit::{Context, IntoElement, ParentElement, Render, Styled, Window, div, px, text};

use crate::panel::variants::Panel;

pub struct ControlPanel;

impl Panel for ControlPanel {
  const NAME: &'static str = "control-panel";
  const WIDTH: f32 = 400.0;
  const HEIGHT: f32 = 600.0;
}

impl Render for ControlPanel {
  fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
    div()
      .h_full()
      .w(px(100.))
      .bg(gpui_kit::black())
      .child(text!("test"))
  }
}
