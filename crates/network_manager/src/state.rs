use std::net::Ipv4Addr;

use cosmic_dbus_networkmanager::{
  config::ip4::AddressData,
  interface::enums::{ActiveConnectionState, DeviceState},
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
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

impl Interface {
  /// NM manages it and it can connect
  pub(crate) fn usable(&self) -> bool {
    !matches!(
      self.state,
      DeviceState::Unmanaged | DeviceState::Unavailable | DeviceState::Unknown
    )
  }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum HotspotBand {
  /// 2.4 GHz
  Bg,
  /// 5 GHz
  A,
}

impl HotspotBand {
  /// its 802-11-wireless.band value
  pub(crate) fn setting(self) -> &'static str {
    match self {
      HotspotBand::Bg => "bg",
      HotspotBand::A => "a",
    }
  }

  /// "a" is 5 GHz, anything else, unset included, 2.4 GHz
  pub(crate) fn from_setting(band: Option<&str>) -> Self {
    match band {
      Some("a") => HotspotBand::A,
      _ => HotspotBand::Bg,
    }
  }
}

/// a device connected to the hotspot
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HotspotClient {
  pub mac: String,
  /// None until it took a DHCP lease
  pub ip: Option<Ipv4Addr>,
  pub hostname: Option<String>,
}

/// a wifi device runs an access point
#[derive(Clone, Debug, PartialEq)]
pub struct HotspotInfo {
  /// up, not still activating
  pub active: bool,
  pub ssid: String,
  pub band: HotspotBand,
  /// the hotspot's saved profile
  pub connection_path: OwnedObjectPath,
  /// the wifi device running it
  pub interface: String,
  /// minutes without clients the Corona Hotspot turns off after, None: never, as any other
  pub auto_off: Option<u32>,
}
