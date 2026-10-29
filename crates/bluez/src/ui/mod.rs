//! The Bluetooth page, its bar button, OSD and dashboard tile

use corona_surface::{
  bar::BarExt, control_center::ControlCenterExt, dashboard::DashboardExt, osd::OsdExt,
};
use gpui_kit::App;

mod osd;
pub mod page;
mod quick_action;
mod widget;

pub(crate) fn register(cx: &mut App) {
  cx.bar_mut().register::<widget::BluetoothButton>();
  cx.control_center_mut().register::<page::BluetoothPanel>();
  cx.dashboard_mut()
    .quick_action::<quick_action::BluetoothToggle>();
  cx.osd_mut().register::<osd::BluetoothOsd>();
  osd::init(cx);
}
