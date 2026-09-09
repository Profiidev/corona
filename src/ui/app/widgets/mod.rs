use gpui_kit::{AnyView, App};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ui::{app::panels::control_panel, bar::Widget};

mod active_window;
pub mod button;
mod workspaces;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WidgetType {
  ControlPanel,
  Workspaces,
  ActiveWindow,
}

impl WidgetType {
  pub fn init(&self, cx: &mut App, display_id: Uuid) -> AnyView {
    match self {
      WidgetType::ControlPanel => control_panel::widget::ControlPanelButton::view(cx, display_id),
      WidgetType::Workspaces => workspaces::widget::Workspaces::view(cx, display_id),
      WidgetType::ActiveWindow => active_window::ActiveWindow::view(cx, display_id),
    }
  }
}
