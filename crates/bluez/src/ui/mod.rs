//! The Bluetooth page, its bar button, OSD and dashboard tile

use crate::BluetoothExt;
use corona_surface::{
  bar::BarExt,
  control_center::{ControlCenterExt, ControlCenterPage},
  dashboard::DashboardExt,
  panel::PanelState,
};
use corona_utils::error::ErrorLogExt;
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
  osd::init(cx);
  open_for_pairing(cx);
}

/// Shows the page for a pairing request that comes in while it is closed: nothing else
/// renders the prompt, and the call hangs until BlueZ gives up
fn open_for_pairing(cx: &mut App) {
  let request = cx.bluetooth().pairing_request.clone();
  cx.observe(&request, |request, cx| {
    if request.read(cx).is_some() && !PanelState::is_open(page::BluetoothPanel::ID, cx) {
      PanelState::show(page::BluetoothPanel::ID, false, cx)
        .log_err()
        .ok();
    }
  })
  .detach();
}
