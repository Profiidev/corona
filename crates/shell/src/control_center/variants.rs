use gpui_kit::{App, Window, assets::IconName};
use std::borrow::Cow;

use crate::control_center::{
  ControlCenterPanel, ControlCenterPanelHandle, audio::AudioPanel, bluetooth::BluetoothPanel,
  brightness::BrightnessPanel, calendar::CalendarPanel, dashboard::DashboardPanel,
  media::MediaPanel, network::NetworkPanel, notifications::NotificationsPanel, power::PowerPanel,
  sysinfo::SysinfoPanel, weather::WeatherPanel,
};
use rust_i18n::t;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlCenterType {
  Dashboard,
  Audio,
  Network,
  Bluetooth,
  Power,
  Brightness,
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
      ControlCenterType::Bluetooth => "bluetooth",
      ControlCenterType::Power => "power",
      ControlCenterType::Brightness => "brightness",
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
      ControlCenterType::Bluetooth => IconName::Bluetooth,
      ControlCenterType::Power => IconName::Zap,
      ControlCenterType::Brightness => IconName::Sun,
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
      ControlCenterType::Bluetooth => t!("app.control_center.bluetooth"),
      ControlCenterType::Power => t!("app.control_center.power"),
      ControlCenterType::Brightness => t!("app.control_center.brightness"),
      ControlCenterType::Notifications => t!("app.control_center.notifications"),
      ControlCenterType::Sysinfo => t!("app.control_center.sysinfo"),
      ControlCenterType::Weather => t!("app.control_center.weather"),
      ControlCenterType::Calendar => t!("app.control_center.calendar"),
      ControlCenterType::Media => t!("app.control_center.media"),
    }
  }

  pub fn iter() -> impl Iterator<Item = ControlCenterType> {
    vec![
      ControlCenterType::Dashboard,
      ControlCenterType::Audio,
      ControlCenterType::Network,
      ControlCenterType::Bluetooth,
      ControlCenterType::Power,
      ControlCenterType::Brightness,
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
      ControlCenterType::Bluetooth => BluetoothPanel::handle(window, cx),
      ControlCenterType::Power => PowerPanel::handle(window, cx),
      ControlCenterType::Brightness => BrightnessPanel::handle(window, cx),
      ControlCenterType::Notifications => NotificationsPanel::handle(window, cx),
      ControlCenterType::Sysinfo => SysinfoPanel::handle(window, cx),
      ControlCenterType::Weather => WeatherPanel::handle(window, cx),
      ControlCenterType::Calendar => CalendarPanel::handle(window, cx),
      ControlCenterType::Media => MediaPanel::handle(window, cx),
    }
  }
}
