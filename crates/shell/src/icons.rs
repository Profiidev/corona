use corona_network_manager::{Interface, InterfaceType};
use corona_pipewire::CaptureKind;
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

pub fn power_profile(name: &str) -> (IconName, String) {
  match name {
    "power-saver" => (IconName::Leaf, "Power saver".into()),
    "balanced" => (IconName::Gauge, "Balanced".into()),
    "performance" => (IconName::Zap, "Performance".into()),
    other => (IconName::Gauge, other.into()),
  }
}

pub fn capture_icon(kind: CaptureKind) -> IconName {
  match kind {
    CaptureKind::Microphone => IconName::Mic,
    CaptureKind::Camera => IconName::Camera,
    CaptureKind::Screen => IconName::ScreenShare,
  }
}

pub fn capture_label(kind: CaptureKind) -> &'static str {
  match kind {
    CaptureKind::Microphone => "Microphone",
    CaptureKind::Camera => "Camera",
    CaptureKind::Screen => "Screen share",
  }
}
