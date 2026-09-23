use gpui_kit::{Context, IntoElement, Render, Window};

use crate::{
  script::{Script, ScriptManagerExt},
  ui::app::control_center::ControlCenterPanel,
};

pub struct DashboardPanel {
  script: Script,
}

impl ControlCenterPanel for DashboardPanel {
  fn init(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
    let script = cx
      .load_plugin_view("test", "test", window)
      .expect("Failed to load the test plugin");

    Self { script }
  }
}

impl Render for DashboardPanel {
  fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
    self.script.view()
  }
}
