use cosmic_dbus_networkmanager::{
  config::ip4::AddressData,
  interface::enums::{ActiveConnectionState, DeviceState},
};
use zbus::zvariant::OwnedObjectPath;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterfaceType {
  Wired,
  Wireless,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Interface {
  pub path: OwnedObjectPath,
  pub name: String,
  pub ip: Option<AddressData>,
  pub kind: InterfaceType,
  pub state: DeviceState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VpnKind {
  Plugin,
  WireGuard,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Vpn {
  pub uuid: String,
  pub name: String,
  pub kind: VpnKind,
  pub state: ActiveConnectionState,
}
