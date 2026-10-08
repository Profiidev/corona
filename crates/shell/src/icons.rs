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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn volume_icons() {
    assert_eq!(volume_icon(1., true), IconName::VolumeOff);
    assert_eq!(volume_icon(0., true), IconName::VolumeOff);
    assert_eq!(volume_icon(0., false), IconName::VolumeX);
    assert_eq!(volume_icon(f32::EPSILON / 2., false), IconName::VolumeX);
    assert_eq!(volume_icon(f32::EPSILON, false), IconName::Volume1);
    assert_eq!(volume_icon(0.49, false), IconName::Volume1);
    assert_eq!(volume_icon(0.5, false), IconName::Volume2);
    assert_eq!(volume_icon(1.5, false), IconName::Volume2);
  }

  #[test]
  fn power_profiles() {
    for (name, icon) in [
      ("power-saver", IconName::Leaf),
      ("balanced", IconName::Gauge),
      ("performance", IconName::Zap),
    ] {
      let (i, label) = power_profile(name);
      assert_eq!(i, icon);
      assert!(!label.is_empty() && !label.starts_with("app."), "{label}");
    }
    let (icon, label) = power_profile("custom");
    assert_eq!(icon, IconName::Gauge);
    assert_eq!(label, "custom");
  }

  #[test]
  fn capture_kinds() {
    let kinds = [
      CaptureKind::Microphone,
      CaptureKind::Camera,
      CaptureKind::Screen,
    ];
    for kind in kinds {
      let label = capture_label(kind);
      assert!(!label.is_empty() && !label.starts_with("app."), "{label}");
    }
    let icons: std::collections::HashSet<_> = kinds.map(capture_icon).into();
    assert_eq!(icons.len(), kinds.len());
  }

  #[test]
  fn no_interface_is_offline() {
    assert_eq!(interface_icon(None), IconName::GlobeOff);
  }
}
