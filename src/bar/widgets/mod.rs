use gpui_kit::{AnyView, App, AppContext, Render};
use serde::{Deserialize, Serialize};

mod button;
mod control_panel;

pub trait Widget: Render {}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WidgetType {
  ControlPanel,
}

impl WidgetType {
  pub fn init(&self, cx: &mut App) -> AnyView {
    cx.new(|_| self.widget()).into()
  }

  fn widget(&self) -> impl Widget {
    match self {
      WidgetType::ControlPanel => control_panel::ControlPanelButton,
    }
  }
}
