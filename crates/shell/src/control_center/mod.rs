mod audio;
mod bluetooth;
pub(crate) mod brightness;
mod calendar;
mod clipboard;
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

use corona_config::{ConfigProvider, ControlCenterConfig};
use corona_surface::panel::{AppPanelExt, Panel};
use gpui_kit::{AnyElement, AnyView, App, AppContext, Context, Entity, Render, Window};

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
    .register::<Standalone<ClipboardPanel>>()
    .register::<Standalone<SysinfoPanel>>()
    .register::<Standalone<WeatherPanel>>()
    .register::<Standalone<CalendarPanel>>()
    .register::<Standalone<MediaPanel>>();
  warn_bad_ids(cx);
}

/// The items `ids` name, each once, and the ids that name none or repeat one
pub(crate) fn resolve<T: PartialEq>(
  ids: &[String],
  parse: impl Fn(&str) -> Option<T>,
) -> (Vec<T>, Vec<&str>) {
  let (mut items, mut bad) = (Vec::new(), Vec::new());
  for id in ids {
    match parse(id) {
      Some(item) if !items.contains(&item) => items.push(item),
      _ => bad.push(id.as_str()),
    }
  }
  (items, bad)
}

/// Logs the sidebar and quick action ids that are skipped, now and whenever they change
fn warn_bad_ids(cx: &mut App) {
  fn warn(config: &ControlCenterConfig) {
    let (_, sidebar) = resolve(&config.sidebar, ControlCenterType::sidebar_page);
    let (_, actions) = resolve(&config.quick_actions, dashboard::toggles::quick_action_id);
    if !sidebar.is_empty() {
      tracing::warn!("control_center.sidebar: skipping unknown or repeated {sidebar:?}");
    }
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
