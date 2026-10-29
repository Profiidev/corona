//! The network and hotspot pages, their bar buttons, the Wi-Fi OSD and the Wi-Fi and
//! airplane mode tiles

use corona_surface::{bar::BarExt, control_center::ControlCenterExt, dashboard::DashboardExt};
use gpui_kit::App;

pub mod hotspot;
mod hotspot_widget;
pub mod icons;
pub mod network;
mod osd;
pub mod quick_action;
mod widget;

pub(crate) fn register(cx: &mut App) {
  cx.bar_mut()
    .register::<widget::NetworkButton>()
    .register::<hotspot_widget::HotspotButton>();
  cx.control_center_mut()
    .register::<network::NetworkPanel>()
    .register::<hotspot::HotspotPanel>();
  cx.dashboard_mut()
    .quick_action::<quick_action::WifiToggle>()
    .quick_action::<quick_action::AirplaneToggle>();
  osd::init(cx);
}
