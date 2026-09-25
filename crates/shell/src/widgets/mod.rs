use corona_config::widget::WidgetType;
use gpui_kit::{AnyView, App};
use uuid::Uuid;

use corona_surface::bar::Widget;

mod active_window;
pub mod button;
mod control_panel;
mod workspaces;

pub fn view(widget: WidgetType, cx: &mut App, display_id: Uuid) -> AnyView {
  match widget {
    WidgetType::ControlCenter => control_panel::ControlCenterButton::view(cx, display_id),
    WidgetType::Workspaces => workspaces::widget::Workspaces::view(cx, display_id),
    WidgetType::ActiveWindow => active_window::ActiveWindow::view(cx, display_id),
  }
}
