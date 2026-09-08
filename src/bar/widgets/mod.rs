use gpui_kit::{AnyView, App, AppContext, Context, Render};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

mod active_window;
mod button;
mod control_panel;
mod workspaces;

pub trait Widget: Render {
  fn init(cx: &mut Context<'_, Self>, display_id: Uuid) -> Self;

  fn view(cx: &mut App, display_id: Uuid) -> AnyView {
    cx.new(|cx| Self::init(cx, display_id)).into()
  }
}

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
      WidgetType::ControlPanel => control_panel::ControlPanelButton::view(cx, display_id),
      WidgetType::Workspaces => workspaces::Workspaces::view(cx, display_id),
      WidgetType::ActiveWindow => active_window::ActiveWindow::view(cx, display_id),
    }
  }
}
