//! Quick actions, sliders and cards of the dashboard

use corona_surface::dashboard::DashboardExt;
use gpui_kit::App;

mod cards;
mod quick_actions;
mod sliders;

pub(crate) fn register_all(cx: &mut App) {
  cx.dashboard_mut()
    .quick_action::<quick_actions::bluetooth::BluetoothToggle>()
    .slider::<sliders::audio::Output>()
    .slider::<sliders::audio::Input>()
    .quick_action::<quick_actions::notifications::DndToggle>()
    .header::<cards::sysinfo::ProfileCard>()
    .card::<cards::mpris::MediaCard>();
}
