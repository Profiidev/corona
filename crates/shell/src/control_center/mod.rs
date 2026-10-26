mod audio;
mod bluetooth;
pub(crate) mod brightness;
mod calendar;
pub(crate) mod dashboard;
mod disks;
mod displays;
mod hotspot;
mod layout;
mod media;
mod nav;
mod network;
mod notifications;
mod panel;
mod power;
mod printers;
pub(crate) mod sysinfo;
pub(crate) mod variants;
mod weather;

use corona_surface::panel::{AppPanelExt, Panel};
use gpui_kit::{AnyElement, AnyView, App, AppContext, Context, Entity, Render, Window};

pub use crate::control_center::{
  audio::AudioPanel,
  bluetooth::BluetoothPanel,
  brightness::BrightnessPanel,
  calendar::CalendarPanel,
  dashboard::DashboardPanel,
  disks::DisksPanel,
  displays::DisplaysPanel,
  hotspot::{HotspotPanel, poll_clients},
  media::MediaPanel,
  network::NetworkPanel,
  notifications::{NotificationsPanel, ReplyInputs},
  panel::{ControlCenter, Standalone},
  power::PowerPanel,
  printers::PrintersPanel,
  sysinfo::SysinfoPanel,
  variants::ControlCenterType,
  weather::WeatherPanel,
};

pub fn register_panels(cx: &mut App) {
  cx.panel()
    .register::<ControlCenter>()
    .register::<Standalone<DashboardPanel>>()
    .register::<Standalone<AudioPanel>>()
    .register::<Standalone<NetworkPanel>>()
    .register::<Standalone<HotspotPanel>>()
    .register::<Standalone<BluetoothPanel>>()
    .register::<Standalone<PrintersPanel>>()
    .register::<Standalone<DisksPanel>>()
    .register::<Standalone<PowerPanel>>()
    .register::<Standalone<BrightnessPanel>>()
    .register::<Standalone<DisplaysPanel>>()
    .register::<Standalone<NotificationsPanel>>()
    .register::<Standalone<SysinfoPanel>>()
    .register::<Standalone<WeatherPanel>>()
    .register::<Standalone<CalendarPanel>>()
    .register::<Standalone<MediaPanel>>();
}

/// The disks panel's job history and drive notifications, which run with the panel closed
pub fn init_disks(cx: &mut App) {
  disks::jobs::init(cx);
  disks::hotplug::init(cx);
}

pub(crate) use disks::jobs::log as disks_log;

#[cfg(test)]
pub(crate) use disks::runtime_keys as disks_runtime_keys;

#[cfg(test)]
pub(crate) fn disks_test_drive(removable: bool) -> corona_udisks::Drive {
  disks::test_drive(removable)
}

pub trait ControlCenterPanel: Render {
  const TYPE: ControlCenterType;
  const HEIGHT: f32 = ControlCenter::HEIGHT;

  fn init(window: &mut Window, cx: &mut Context<'_, Self>) -> Self;

  fn buttons(&mut self, _cx: &mut Context<Self>) -> Vec<AnyElement> {
    vec![]
  }

  fn handle(window: &mut Window, cx: &mut App) -> Box<dyn ControlCenterPanelHandle> {
    Box::new(cx.new(|cx| Self::init(window, cx)))
  }
}

pub trait ControlCenterPanelHandle {
  fn view(&self) -> AnyView;
  fn buttons(&self, cx: &mut App) -> Vec<AnyElement>;
}

impl<T: ControlCenterPanel> ControlCenterPanelHandle for Entity<T> {
  fn view(&self) -> AnyView {
    self.clone().into()
  }

  fn buttons(&self, cx: &mut App) -> Vec<AnyElement> {
    self.update(cx, |page, cx| page.buttons(cx))
  }
}
