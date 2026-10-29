//! The power page, the battery and power buttons, the profile OSD and tile, and the low
//! battery warnings

use corona_surface::{bar::BarExt, control_center::ControlCenterExt, dashboard::DashboardExt};
use gpui_kit::App;

mod battery;
pub mod icons;
mod osd;
pub mod page;
mod quick_action;
mod warnings;
mod widget;

pub(crate) fn register(cx: &mut App) {
  cx.bar_mut()
    .register::<widget::PowerButton>()
    .register::<battery::BatteryButton>();
  cx.control_center_mut().register::<page::PowerPanel>();
  cx.dashboard_mut()
    .quick_action::<quick_action::PowerProfileToggle>();
  osd::init(cx);
  warnings::init(cx);
}
