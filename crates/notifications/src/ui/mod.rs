//! The notifications page, its bar button, the popups, the do not disturb OSD and tile

use corona_surface::{bar::BarExt, control_center::ControlCenterExt, dashboard::DashboardExt};
use gpui_kit::App;

mod osd;
pub mod overlay;
pub mod page;
mod quick_action;
mod widget;

pub use page::{NotificationsPanel, ReplyInputs};

pub(crate) fn register(cx: &mut App) {
  cx.bar_mut().register::<widget::NotificationsButton>();
  cx.control_center_mut().register::<NotificationsPanel>();
  cx.dashboard_mut().quick_action::<quick_action::DndToggle>();
  osd::init(cx);
  overlay::init(cx);
}
