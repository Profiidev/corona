use gpui_kit::{App, Window, assets::IconName};

use crate::control_center::{
  ControlCenterPanel, ControlCenterPanelHandle, audio::AudioPanel, bluetooth::BluetoothPanel,
  brightness::BrightnessPanel, calendar::CalendarPanel, dashboard::DashboardPanel,
  media::MediaPanel, network::NetworkPanel, notifications::NotificationsPanel, power::PowerPanel,
  sysinfo::SysinfoPanel, weather::WeatherPanel,
};

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

  pub fn title(&self) -> &'static str {
    match self {
      ControlCenterType::Dashboard => "Dashboard",
      ControlCenterType::Audio => "Audio",
      ControlCenterType::Network => "Network",
      ControlCenterType::Bluetooth => "Bluetooth",
      ControlCenterType::Power => "Power",
      ControlCenterType::Brightness => "Brightness",
      ControlCenterType::Notifications => "Notifications",
      ControlCenterType::Sysinfo => "System",
      ControlCenterType::Weather => "Weather",
      ControlCenterType::Calendar => "Calendar",
      ControlCenterType::Media => "Media",
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
