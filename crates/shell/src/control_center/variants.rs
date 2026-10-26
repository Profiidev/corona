use corona_config::ControlCenterConfig;
use gpui_kit::{App, Window, assets::IconName};
use std::borrow::Cow;

use crate::control_center::{
  ControlCenterPanel, ControlCenterPanelHandle, audio::AudioPanel, bluetooth::BluetoothPanel,
  brightness::BrightnessPanel, calendar::CalendarPanel, dashboard::DashboardPanel,
  disks::DisksPanel, displays::DisplaysPanel, hotspot::HotspotPanel, media::MediaPanel,
  network::NetworkPanel, notifications::NotificationsPanel, power::PowerPanel,
  printers::PrintersPanel, resolve, sysinfo::SysinfoPanel, weather::WeatherPanel,
};
use rust_i18n::t;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlCenterType {
  Dashboard,
  Audio,
  Network,
  Hotspot,
  Bluetooth,
  Printers,
  Disks,
  Power,
  Brightness,
  Displays,
  Notifications,
  Sysinfo,
  Weather,
  Calendar,
  Media,
}

impl ControlCenterType {
  pub const fn as_str(&self) -> &'static str {
    match self {
      ControlCenterType::Dashboard => "dashboard",
      ControlCenterType::Audio => "audio",
      ControlCenterType::Network => "network",
      ControlCenterType::Hotspot => "hotspot",
      ControlCenterType::Bluetooth => "bluetooth",
      ControlCenterType::Printers => "printers",
      ControlCenterType::Disks => "disks",
      ControlCenterType::Power => "power",
      ControlCenterType::Brightness => "brightness",
      ControlCenterType::Displays => "displays",
      ControlCenterType::Notifications => "notifications",
      ControlCenterType::Sysinfo => "sysinfo",
      ControlCenterType::Weather => "weather",
      ControlCenterType::Calendar => "calendar",
      ControlCenterType::Media => "media",
    }
  }

  pub fn icon(&self) -> IconName {
    match self {
      ControlCenterType::Dashboard => IconName::LayoutDashboard,
      ControlCenterType::Audio => IconName::Volume2,
      ControlCenterType::Network => IconName::Wifi,
      ControlCenterType::Hotspot => IconName::RadioTower,
      ControlCenterType::Bluetooth => IconName::Bluetooth,
      ControlCenterType::Printers => IconName::Printer,
      ControlCenterType::Disks => IconName::HardDrive,
      ControlCenterType::Power => IconName::Zap,
      ControlCenterType::Brightness => IconName::Sun,
      ControlCenterType::Displays => IconName::Monitor,
      ControlCenterType::Notifications => IconName::Bell,
      ControlCenterType::Sysinfo => IconName::Activity,
      ControlCenterType::Weather => IconName::CloudSun,
      ControlCenterType::Calendar => IconName::CalendarDays,
      ControlCenterType::Media => IconName::Music,
    }
  }

  pub fn title(&self) -> Cow<'static, str> {
    match self {
      ControlCenterType::Dashboard => t!("app.control_center.dashboard"),
      ControlCenterType::Audio => t!("app.control_center.audio"),
      ControlCenterType::Network => t!("app.control_center.network"),
      ControlCenterType::Hotspot => t!("app.control_center.hotspot"),
      ControlCenterType::Bluetooth => t!("app.control_center.bluetooth"),
      ControlCenterType::Printers => t!("app.control_center.printers"),
      ControlCenterType::Disks => t!("app.control_center.disks"),
      ControlCenterType::Power => t!("app.control_center.power"),
      ControlCenterType::Brightness => t!("app.control_center.brightness"),
      ControlCenterType::Displays => t!("app.control_center.displays"),
      ControlCenterType::Notifications => t!("app.control_center.notifications"),
      ControlCenterType::Sysinfo => t!("app.control_center.sysinfo"),
      ControlCenterType::Weather => t!("app.control_center.weather"),
      ControlCenterType::Calendar => t!("app.control_center.calendar"),
      ControlCenterType::Media => t!("app.control_center.media"),
    }
  }

  pub fn from_id(id: &str) -> Option<ControlCenterType> {
    Self::iter().find(|p| p.as_str() == id)
  }

  /// The sidebar's pages: the dashboard, then the configured ones
  pub fn sidebar(config: &ControlCenterConfig) -> Vec<ControlCenterType> {
    let (pages, _) = resolve(&config.sidebar, Self::sidebar_page);
    std::iter::once(ControlCenterType::Dashboard)
      .chain(pages)
      .collect()
  }

  /// A configurable sidebar page, the dashboard is always there
  pub(crate) fn sidebar_page(id: &str) -> Option<ControlCenterType> {
    Self::from_id(id).filter(|p| *p != ControlCenterType::Dashboard)
  }

  pub fn iter() -> impl Iterator<Item = ControlCenterType> {
    vec![
      ControlCenterType::Dashboard,
      ControlCenterType::Audio,
      ControlCenterType::Network,
      ControlCenterType::Hotspot,
      ControlCenterType::Bluetooth,
      ControlCenterType::Printers,
      ControlCenterType::Disks,
      ControlCenterType::Power,
      ControlCenterType::Brightness,
      ControlCenterType::Displays,
      ControlCenterType::Notifications,
      ControlCenterType::Sysinfo,
      ControlCenterType::Weather,
      ControlCenterType::Calendar,
      ControlCenterType::Media,
    ]
    .into_iter()
  }

  pub fn handle(&self, window: &mut Window, cx: &mut App) -> Box<dyn ControlCenterPanelHandle> {
    match self {
      ControlCenterType::Dashboard => DashboardPanel::handle(window, cx),
      ControlCenterType::Audio => AudioPanel::handle(window, cx),
      ControlCenterType::Network => NetworkPanel::handle(window, cx),
      ControlCenterType::Hotspot => HotspotPanel::handle(window, cx),
      ControlCenterType::Bluetooth => BluetoothPanel::handle(window, cx),
      ControlCenterType::Printers => PrintersPanel::handle(window, cx),
      ControlCenterType::Disks => DisksPanel::handle(window, cx),
      ControlCenterType::Power => PowerPanel::handle(window, cx),
      ControlCenterType::Brightness => BrightnessPanel::handle(window, cx),
      ControlCenterType::Displays => DisplaysPanel::handle(window, cx),
      ControlCenterType::Notifications => NotificationsPanel::handle(window, cx),
      ControlCenterType::Sysinfo => SysinfoPanel::handle(window, cx),
      ControlCenterType::Weather => WeatherPanel::handle(window, cx),
      ControlCenterType::Calendar => CalendarPanel::handle(window, cx),
      ControlCenterType::Media => MediaPanel::handle(window, cx),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn iter_is_complete() {
    let all: Vec<_> = ControlCenterType::iter().collect();
    for page in &all {
      // a new variant fails to compile here until it is listed in `iter`
      match page {
        ControlCenterType::Dashboard
        | ControlCenterType::Audio
        | ControlCenterType::Network
        | ControlCenterType::Hotspot
        | ControlCenterType::Bluetooth
        | ControlCenterType::Printers
        | ControlCenterType::Disks
        | ControlCenterType::Power
        | ControlCenterType::Brightness
        | ControlCenterType::Displays
        | ControlCenterType::Notifications
        | ControlCenterType::Sysinfo
        | ControlCenterType::Weather
        | ControlCenterType::Calendar
        | ControlCenterType::Media => {}
      }
    }
    assert_eq!(all.len(), 15);
    assert_eq!(all[0], ControlCenterType::Dashboard);
  }

  #[test]
  fn ids_titles_icons_unique() {
    let count = ControlCenterType::iter().count();
    let unique = |mut v: Vec<String>| {
      v.sort();
      v.dedup();
      v.len()
    };
    let ids: Vec<_> = ControlCenterType::iter()
      .map(|p| p.as_str().to_string())
      .collect();
    // ids are panel names, the control center itself is another panel
    assert!(!ids.iter().any(|id| id == "control_center"));
    assert_eq!(unique(ids), count);
    let titles = ControlCenterType::iter().map(|p| p.title().to_string());
    assert_eq!(unique(titles.collect()), count);
    let icons = ControlCenterType::iter().map(|p| format!("{:?}", p.icon()));
    assert_eq!(unique(icons.collect()), count);
  }

  #[test]
  fn sidebar_follows_config() {
    let default = ControlCenterConfig::default();
    assert_eq!(
      ControlCenterType::sidebar(&default),
      ControlCenterType::iter().collect::<Vec<_>>()
    );
    let config = ControlCenterConfig {
      sidebar: ["media", "dashboard", "nope", "audio", "media"]
        .map(String::from)
        .to_vec(),
      ..default
    };
    assert_eq!(
      ControlCenterType::sidebar(&config),
      [
        ControlCenterType::Dashboard,
        ControlCenterType::Media,
        ControlCenterType::Audio
      ]
    );
  }
}
