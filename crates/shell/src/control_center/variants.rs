use gpui_kit::{App, Window, assets::IconName};

use crate::control_center::{
  ControlCenterPanel, ControlCenterPanelHandle, audio::AudioPanel, dashboard::DashboardPanel,
  media::MediaPanel, network::NetworkPanel,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlCenterType {
  Dashboard,
  Audio,
  Network,
  Media,
}

impl ControlCenterType {
  pub fn as_str(&self) -> &'static str {
    match self {
      ControlCenterType::Dashboard => "dashboard",
      ControlCenterType::Audio => "audio",
      ControlCenterType::Network => "network",
      ControlCenterType::Media => "media",
    }
  }

  pub fn icon(&self) -> IconName {
    match self {
      ControlCenterType::Dashboard => IconName::LayoutDashboard,
      ControlCenterType::Audio => IconName::Volume2,
      ControlCenterType::Network => IconName::Wifi,
      ControlCenterType::Media => IconName::Music,
    }
  }

  pub fn title(&self) -> &'static str {
    match self {
      ControlCenterType::Dashboard => "Dashboard",
      ControlCenterType::Audio => "Audio",
      ControlCenterType::Network => "Network",
      ControlCenterType::Media => "Media",
    }
  }

  pub fn iter() -> impl Iterator<Item = ControlCenterType> {
    vec![
      ControlCenterType::Dashboard,
      ControlCenterType::Audio,
      ControlCenterType::Network,
      ControlCenterType::Media,
    ]
    .into_iter()
  }

  pub fn handle(&self, window: &mut Window, cx: &mut App) -> Box<dyn ControlCenterPanelHandle> {
    match self {
      ControlCenterType::Dashboard => DashboardPanel::handle(window, cx),
      ControlCenterType::Audio => AudioPanel::handle(window, cx),
      ControlCenterType::Network => NetworkPanel::handle(window, cx),
      ControlCenterType::Media => MediaPanel::handle(window, cx),
    }
  }
}
