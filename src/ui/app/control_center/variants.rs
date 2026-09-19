use gpui_kit::assets::IconName;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlCenterType {
  Dashboard,
  Audio,
  Network,
}

impl ControlCenterType {
  pub fn as_str(&self) -> &'static str {
    match self {
      ControlCenterType::Dashboard => "dashboard",
      ControlCenterType::Audio => "audio",
      ControlCenterType::Network => "network",
    }
  }

  pub fn icon(&self) -> IconName {
    match self {
      ControlCenterType::Dashboard => IconName::LayoutDashboard,
      ControlCenterType::Audio => IconName::Volume2,
      ControlCenterType::Network => IconName::Wifi,
    }
  }

  pub fn title(&self) -> &'static str {
    match self {
      ControlCenterType::Dashboard => "Dashboard",
      ControlCenterType::Audio => "Audio",
      ControlCenterType::Network => "Network",
    }
  }

  pub fn iter() -> impl Iterator<Item = ControlCenterType> {
    vec![
      ControlCenterType::Dashboard,
      ControlCenterType::Audio,
      ControlCenterType::Network,
    ]
    .into_iter()
  }
}
