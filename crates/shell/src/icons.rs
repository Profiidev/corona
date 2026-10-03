use corona_network_manager::{Interface, InterfaceType};
use gpui_kit::assets::IconName;

pub fn volume_icon(volume: f32, muted: bool) -> IconName {
  if muted {
    IconName::VolumeOff
  } else if volume < f32::EPSILON {
    IconName::VolumeX
  } else if volume < 0.5 {
    IconName::Volume1
  } else {
    IconName::Volume2
  }
}

pub fn interface_icon(primary: Option<&Interface>) -> IconName {
  match primary {
    None => IconName::GlobeOff,
    Some(i) if i.kind == InterfaceType::Wired => IconName::EthernetPort,
    Some(_) => IconName::Wifi,
  }
}
