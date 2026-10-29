use corona_network_manager::{Interface, InterfaceType};
use gpui_kit::assets::IconName;
use rust_i18n::t;
use std::borrow::Cow;

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

#[cfg(test)]
mod tests {
  use super::*;

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
  fn no_interface_is_offline() {
    assert_eq!(interface_icon(None), IconName::GlobeOff);
  }
}
