use corona_network_manager::{Interface, InterfaceType};
use corona_pipewire::CaptureKind;
use gpui_kit::assets::IconName;
use rust_i18n::t;
use std::borrow::Cow;

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

pub fn power_profile(name: &str) -> (IconName, Cow<'static, str>) {
  match name {
    "power-saver" => (IconName::Leaf, t!("app.power.profile.power_saver")),
    "balanced" => (IconName::Gauge, t!("app.power.profile.balanced")),
    "performance" => (IconName::Zap, t!("app.power.profile.performance")),
    other => (IconName::Gauge, other.to_string().into()),
  }
}

pub fn capture_icon(kind: CaptureKind) -> IconName {
  match kind {
    CaptureKind::Microphone => IconName::Mic,
    CaptureKind::Camera => IconName::Camera,
    CaptureKind::Screen => IconName::ScreenShare,
  }
}

pub fn capture_label(kind: CaptureKind) -> Cow<'static, str> {
  match kind {
    CaptureKind::Microphone => t!("app.privacy.microphone"),
    CaptureKind::Camera => t!("app.privacy.camera"),
    CaptureKind::Screen => t!("app.privacy.screen_share"),
  }
}
