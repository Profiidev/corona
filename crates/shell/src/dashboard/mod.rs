//! Quick actions, sliders and cards of the dashboard

use corona_surface::dashboard::DashboardExt;
use gpui_kit::{App, AppContext};

mod cards;
mod session;
pub(crate) mod quick_actions;
pub(crate) mod sliders;

pub(crate) fn register_all(cx: &mut App) {
  cx.dashboard_mut()
    .button(|_, cx| cx.new(session::PowerMenu::new).into());
  cx.dashboard_mut()
    .quick_action::<quick_actions::notifications::DndToggle>()
    .slider::<sliders::audio::Output>()
    .slider::<sliders::audio::Input>()
    .header::<cards::sysinfo::ProfileCard>()
    .quick_action::<quick_actions::bluetooth::BluetoothToggle>()
    .quick_action::<quick_actions::network::WifiToggle>()
    .quick_action::<quick_actions::network::AirplaneToggle>()
    .quick_action::<quick_actions::power::PowerProfileToggle>()
    .card::<cards::clock::ClockCard>()
    .slider::<sliders::brightness::Brightness>()
    .quick_action::<quick_actions::brightness::NightlightToggle>()
    .card::<cards::mpris::MediaCard>()
    .quick_action::<quick_actions::idle::StayAwakeToggle>();
}
