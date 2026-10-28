mod audio;
mod bluetooth;
pub(crate) mod brightness;
mod calendar;
mod clipboard;
pub(crate) mod dashboard;
mod disks;
mod displays;
mod hotspot;
mod media;
mod network;
mod notifications;
mod power;
mod printers;
pub(crate) mod sysinfo;
mod weather;

use corona_config::{ConfigProvider, ControlCenterConfig};
use corona_surface::{
  control_center::{ControlCenter, ControlCenterExt, resolve},
  panel::AppPanelExt,
};
use gpui_kit::App;

pub use crate::control_center::{
  audio::AudioPanel,
  bluetooth::BluetoothPanel,
  brightness::BrightnessPanel,
  calendar::CalendarPanel,
  clipboard::ClipboardPanel,
  dashboard::DashboardPanel,
  disks::DisksPanel,
  displays::DisplaysPanel,
  hotspot::{HotspotPanel, poll_clients},
  media::MediaPanel,
  network::NetworkPanel,
  notifications::{NotificationsPanel, ReplyInputs},
  power::PowerPanel,
  printers::PrintersPanel,
  sysinfo::SysinfoPanel,
  weather::WeatherPanel,
};

pub fn register_panels(cx: &mut App) {
  cx.panel().register::<ControlCenter>();
  cx.control_center_mut()
    .register::<DashboardPanel>()
    .register::<AudioPanel>()
    .register::<NetworkPanel>()
    .register::<HotspotPanel>()
    .register::<BluetoothPanel>()
    .register::<PrintersPanel>()
    .register::<DisksPanel>()
    .register::<PowerPanel>()
    .register::<BrightnessPanel>()
    .register::<DisplaysPanel>()
    .register::<NotificationsPanel>()
    .register::<ClipboardPanel>()
    .register::<SysinfoPanel>()
    .register::<WeatherPanel>()
    .register::<CalendarPanel>()
    .register::<MediaPanel>();
  corona_surface::control_center::warn_bad_ids(cx);
  warn_bad_actions(cx);
}

/// Logs the quick action ids that are skipped, now and whenever they change
fn warn_bad_actions(cx: &mut App) {
  fn warn(config: &ControlCenterConfig) {
    let (_, actions) = resolve(&config.quick_actions, dashboard::toggles::quick_action_id);
    if !actions.is_empty() {
      tracing::warn!("control_center.quick_actions: skipping unknown or repeated {actions:?}");
    }
  }
  warn(&cx.config().control_center);
  corona_config::observe_section(cx, |c| &c.control_center, |config, _| warn(config));
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
