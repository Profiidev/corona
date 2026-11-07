use corona_network_manager::{Interface, InterfaceType, WifiNetwork, WifiStatus};
use gpui_kit::assets::IconName;

pub fn interface_icon(primary: Option<&Interface>) -> IconName {
  match primary {
    None => IconName::GlobeOff,
    Some(i) if i.kind == InterfaceType::Wired => IconName::EthernetPort,
    Some(_) => IconName::Wifi,
  }
}

/// The wifi icon for a signal strength in percent
pub fn signal_icon(strength: u8) -> IconName {
  match strength {
    0..25 => IconName::WifiZero,
    25..50 => IconName::WifiLow,
    50..75 => IconName::WifiHigh,
    75.. => IconName::Wifi,
  }
}

/// `interface_icon`, with the connected network's signal when the primary is wifi
pub fn primary_icon(primary: Option<&Interface>, networks: &[WifiNetwork]) -> IconName {
  let icon = interface_icon(primary);
  if icon != IconName::Wifi {
    return icon;
  }
  (networks.iter())
    .find(|n| n.status == WifiStatus::Connected)
    .map_or(icon, |n| signal_icon(n.strength))
}

#[cfg(test)]
mod tests {
  use super::*;

  fn network(strength: u8, status: WifiStatus) -> WifiNetwork {
    WifiNetwork {
      ssid: "a".into(),
      raw_ssid: b"a".to_vec(),
      strength,
      secured: false,
      enterprise: false,
      status,
    }
  }

  fn interface(kind: InterfaceType) -> Interface {
    Interface {
      path: zbus::zvariant::OwnedObjectPath::try_from("/").unwrap(),
      name: "x".into(),
      ip: None,
      kind,
      state: corona_network_manager::DeviceState::Activated,
    }
  }

  #[test]
  fn signal_steps() {
    assert_eq!(signal_icon(0), IconName::WifiZero);
    assert_eq!(signal_icon(24), IconName::WifiZero);
    assert_eq!(signal_icon(25), IconName::WifiLow);
    assert_eq!(signal_icon(60), IconName::WifiHigh);
    assert_eq!(signal_icon(75), IconName::Wifi);
  }

  #[test]
  fn primary_wifi_shows_its_signal() {
    let wifi = interface(InterfaceType::Wireless);
    let nets = [
      network(90, WifiStatus::Saved),
      network(30, WifiStatus::Connected),
    ];
    assert_eq!(primary_icon(Some(&wifi), &nets), IconName::WifiLow);
    // nothing connected yet: the plain icon
    assert_eq!(primary_icon(Some(&wifi), &nets[..1]), IconName::Wifi);
    // wired and offline ignore the networks
    let wired = interface(InterfaceType::Wired);
    assert_eq!(primary_icon(Some(&wired), &nets), IconName::EthernetPort);
    assert_eq!(primary_icon(None, &nets), IconName::GlobeOff);
  }

  #[test]
  fn no_interface_is_offline() {
    assert_eq!(interface_icon(None), IconName::GlobeOff);
  }
}
