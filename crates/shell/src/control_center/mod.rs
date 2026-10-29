pub(crate) mod brightness;
mod clipboard;
pub(crate) mod dashboard;
mod disks;
mod displays;
mod hotspot;
mod network;
mod power;

use corona_surface::{
  control_center::{ControlCenter, ControlCenterExt},
  panel::AppPanelExt,
};
use gpui_kit::App;

pub use crate::control_center::{
  brightness::BrightnessPanel,
  clipboard::ClipboardPanel,
  dashboard::DashboardPanel,
  disks::DisksPanel,
  displays::DisplaysPanel,
  hotspot::{HotspotPanel, poll_clients},
  network::NetworkPanel,
  power::PowerPanel,
};

pub fn register_panels(cx: &mut App) {
  cx.panel().register::<ControlCenter>();
  cx.control_center_mut()
    .register::<DashboardPanel>()
    .register::<NetworkPanel>()
    .register::<HotspotPanel>()
    .register::<DisksPanel>()
    .register::<PowerPanel>()
    .register::<BrightnessPanel>()
    .register::<DisplaysPanel>()
    .register::<ClipboardPanel>();
  corona_surface::control_center::warn_bad_ids(cx);
  dashboard::register(cx);
  corona_surface::dashboard::warn_bad_ids(cx);
}

/// The disks panel's job history and drive notifications, which run with the panel closed
pub fn init_disks(cx: &mut App) {
  disks::jobs::init(cx);
  disks::hotplug::init(cx);
}

/// Low battery notifications, which run with the panel closed
pub fn init_power(cx: &mut App) {
  power::warnings::init(cx);
}

pub(crate) use disks::jobs::log as disks_log;

#[cfg(test)]
pub(crate) use disks::runtime_keys as disks_runtime_keys;

#[cfg(test)]
pub(crate) fn disks_test_drive(removable: bool) -> corona_udisks::Drive {
  disks::test_drive(removable)
}
