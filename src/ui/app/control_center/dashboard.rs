use gpui_kit::{Context, IntoElement, Render, Window};

use crate::ui::app::control_center::ControlCenterPanel;

pub struct DashboardPanel {}

impl ControlCenterPanel for DashboardPanel {
  fn init(_cx: &mut Context<'_, Self>) -> Self {
    Self {}
  }
}

impl Render for DashboardPanel {
  fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
    "dashboard"
  }
}
