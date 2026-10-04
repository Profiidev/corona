use corona_utils::error::ErrorLogExt;
use gpui_kit::{Context, IntoElement, Render, Window};

use corona_script::{Script, ScriptManagerExt};

use crate::control_center::{ControlCenterPanel, variants::ControlCenterType};

pub struct DashboardPanel {
  script: Option<Script>,
}

impl ControlCenterPanel for DashboardPanel {
  const TYPE: ControlCenterType = ControlCenterType::Dashboard;

  fn init(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
    let script = cx.load_plugin_view("test", "test", window).log_err().ok();

    Self { script }
  }
}

impl Render for DashboardPanel {
  fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
    if let Some(script) = self.script.as_ref() {
      script.view().into_any_element()
    } else {
      "Failed to load the test plugin".into_any_element()
    }
  }
}
