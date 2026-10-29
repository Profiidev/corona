use corona_network_manager::{Interface, InterfaceType};
use gpui_kit::assets::IconName;

pub fn interface_icon(primary: Option<&Interface>) -> IconName {
  match primary {
    None => IconName::GlobeOff,
    Some(i) if i.kind == InterfaceType::Wired => IconName::EthernetPort,
    Some(_) => IconName::Wifi,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn no_interface_is_offline() {
    assert_eq!(interface_icon(None), IconName::GlobeOff);
  }
}
