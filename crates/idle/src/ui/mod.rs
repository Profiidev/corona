//! Stay awake's OSD and dashboard tile

use corona_surface::dashboard::DashboardExt;
use gpui_kit::App;

mod osd;
mod quick_action;

pub(crate) fn register(cx: &mut App) {
  cx.dashboard_mut()
    .quick_action::<quick_action::StayAwakeToggle>();
  osd::init(cx);
}
