use std::{cmp::Reverse, collections::HashMap, ops::Deref};

use anyhow::Result;
use corona_utils::error::ErrorLogExt;
use cosmic_dbus_networkmanager::{
  access_point::AccessPoint,
  device::{Device, wireless::WirelessDevice},
  interface::{
    active_connection::ActiveConnectionProxy,
    device::{DeviceProxy, wireless::WirelessDeviceProxy},
    enums::{
      ActiveConnectionState, ApFlags, ApSecurityFlags, DeviceState, DeviceType, NmConnectivityState,
    },
    settings::{SettingsProxy, connection::ConnectionSettingsProxy},
  },
  nm::NetworkManager,
  settings::connection::{Connection as NmConnection, Settings},
};
use zbus::{Connection, proxy::CacheProperties, zvariant::OwnedObjectPath};

use crate::state::{Interface, InterfaceType, Vpn, VpnKind};

const VPN_TYPE: &str = "vpn";
const WIREGUARD_TYPE: &str = "wireguard";

pub struct Snapshot {
  pub interfaces: Vec<Interface>,
  pub primary_interface: Option<Interface>,
  pub connectivity: NmConnectivityState,
  pub wifi_supported: bool,
  pub wifi_enabled: bool,
  pub primary_wifi: Option<Interface>,
  pub wifi_networks: Vec<WifiNetwork>,
  pub vpns: Vec<Vpn>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum WifiStatus {
  Connected,
  NeedAuth,
  Connecting,
  Saved,
  New,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WifiNetwork {
  pub ssid: String,
  pub strength: u8,
  /// needs a password or 802.1X credentials, open and OWE networks don't
  pub secured: bool,
  /// 802.1X: a new profile needs `connect_enterprise_wifi`, not `connect_wifi`
  pub enterprise: bool,
  pub status: WifiStatus,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WifiFailure {
  /// None when the attempt started and failed between two snapshots
  pub ssid: Option<String>,
  pub reason: FailReason,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailReason {
  /// no password given or the password was wrong
  NoSecrets,
  SsidNotFound,
  /// raw NMDeviceStateReason
  Other(u32),
}

impl From<u32> for FailReason {
  fn from(reason: u32) -> Self {
    // NM_DEVICE_STATE_REASON_NO_SECRETS / _SSID_NOT_FOUND
    match reason {
      7 => FailReason::NoSecrets,
      53 => FailReason::SsidNotFound,
      other => FailReason::Other(other),
    }
  }
}

pub async fn snapshot(conn: &Connection) -> Result<Snapshot> {
  let nm = NetworkManager::new(conn).await?;

  let mut interfaces = Vec::new();
  for device in nm.devices().await? {
    if let Ok(Some(i)) = read_device(device).await.log_err() {
      interfaces.push(i);
    }
  }

  let primary_path = nm.primary_connection().await?;
  let primary_devices = if primary_path == OwnedObjectPath::default() {
    Vec::new()
  } else {
    ActiveConnectionProxy::builder(conn)
      .path(primary_path)?
      .cache_properties(CacheProperties::No)
      .build()
      .await?
      .devices()
      .await?
  };

  let primary_interface = interfaces
    .iter()
    .find(|i| primary_devices.contains(&i.path))
    .or_else(|| {
      interfaces
        .iter()
        .filter(|i| i.state == DeviceState::Activated)
        .min_by_key(|i| i.kind != InterfaceType::Wired)
    })
    .cloned();

  let wifi = || {
    interfaces
      .iter()
      .filter(|i| i.kind == InterfaceType::Wireless)
  };
  let primary_wifi = primary_interface
    .clone()
    .filter(|i| i.kind == InterfaceType::Wireless)
    .or_else(|| wifi().find(|i| i.state == DeviceState::Activated).cloned())
    .or_else(|| {
      wifi()
        .find(|i| {
          !matches!(
            i.state,
            DeviceState::Unmanaged | DeviceState::Unavailable | DeviceState::Unknown
          )
        })
        .cloned()
    });

  let wifi_supported = interfaces
    .iter()
    .any(|i| i.kind == InterfaceType::Wireless && i.state != DeviceState::Unmanaged);

  let wifi_networks = match &primary_wifi {
    Some(device) => wifi_networks(conn, device)
      .await
      .log_err()
      .unwrap_or_default(),
    None => Vec::new(),
  };

  Ok(Snapshot {
    interfaces,
    primary_interface,
    connectivity: nm.connectivity().await?,
    wifi_supported,
    wifi_enabled: nm.wireless_enabled().await?,
    primary_wifi,
    wifi_networks,
    vpns: vpns(conn, &nm).await.log_err().unwrap_or_default(),
  })
}

async fn vpns(conn: &Connection, nm: &NetworkManager<'_>) -> Result<Vec<Vpn>> {
  let mut active = HashMap::new();
  for connection in nm.active_connections().await? {
    if let (Ok(uuid), Ok(state)) = (connection.uuid().await, connection.state().await) {
      active.insert(uuid, state);
    }
  }

  let settings = SettingsProxy::builder(conn)
    .cache_properties(CacheProperties::No)
    .build()
    .await?;
  let mut vpns = Vec::new();
  for path in settings.list_connections().await? {
    let Ok(profile) = profile(conn, path).await else {
      continue;
    };
    let Some(connection) = profile.connection else {
      continue;
    };
    let kind = match connection.type_.as_deref() {
      Some(VPN_TYPE) => VpnKind::Plugin,
      Some(WIREGUARD_TYPE) => VpnKind::WireGuard,
      _ => continue,
    };
    let (Some(name), Some(uuid)) = (connection.id, connection.uuid) else {
      continue;
    };
    vpns.push(Vpn {
      state: active
        .get(&uuid)
        .copied()
        .unwrap_or(ActiveConnectionState::Deactivated),
      uuid,
      name,
      kind,
    });
  }
  vpns.sort_by(|a, b| a.name.cmp(&b.name));
  Ok(vpns)
}

async fn read_device(device: Device<'_>) -> Result<Option<Interface>> {
  let kind = match device.device_type().await? {
    DeviceType::Ethernet => InterfaceType::Wired,
    DeviceType::Wifi => InterfaceType::Wireless,
    _ => return Ok(None),
  };

  let config = device.ip4_config().await?;
  let ip = if config.inner().path() == OwnedObjectPath::default().deref() {
    None
  } else {
    config.address_data().await?.first().copied()
  };

  Ok(Some(Interface {
    path: device.inner().path().to_owned().into(),
    name: device.interface().await?,
    ip,
    kind,
    state: device.state().await?,
  }))
}

async fn wifi_networks(conn: &Connection, device: &Interface) -> Result<Vec<WifiNetwork>> {
  let wireless: WirelessDevice = WirelessDeviceProxy::builder(conn)
    .path(device.path.clone())?
    .cache_properties(CacheProperties::No)
    .build()
    .await?
    .into();
  let device: Device = DeviceProxy::builder(conn)
    .path(device.path.clone())?
    .cache_properties(CacheProperties::No)
    .build()
    .await?
    .into();

  let available = device.available_connections().await?;

  let mut saved = Vec::new();
  for conn in available {
    if let Ok(Some(ssid)) = saved_ssid(&conn).await {
      saved.push(ssid);
    }
  }

  let active = wireless.active_access_point().await?;
  let mut networks: Vec<WifiNetwork> = Vec::new();
  for ap in wireless.get_access_points().await? {
    let active_status = match device.state().await? {
      DeviceState::Activated => Some(WifiStatus::Connected),
      DeviceState::NeedAuth => Some(WifiStatus::NeedAuth),
      DeviceState::Prepare
      | DeviceState::Config
      | DeviceState::IpConfig
      | DeviceState::IpCheck
      | DeviceState::Secondaries => Some(WifiStatus::Connecting),
      _ => None,
    }
    .filter(|_| ap.inner().path() == active.inner().path());
    // an AP can vanish between listing and reading, skip it
    let Ok(AccessPointInfo {
      ssid,
      strength,
      secured,
      enterprise,
    }) = read_access_point(&ap).await
    else {
      continue;
    };
    if ssid.is_empty() {
      continue;
    }

    let status = if let Some(status) = active_status {
      status
    } else if saved.contains(&ssid) {
      WifiStatus::Saved
    } else {
      WifiStatus::New
    };
    let ssid = String::from_utf8_lossy(&ssid).into_owned();
    match networks.iter_mut().find(|n| n.ssid == ssid) {
      Some(n) => {
        n.strength = n.strength.max(strength);
        n.secured |= secured;
        n.enterprise |= enterprise;
        n.status = n.status.min(status);
      }
      None => networks.push(WifiNetwork {
        ssid,
        strength,
        secured,
        enterprise,
        status,
      }),
    }
  }

  networks.sort_by_key(|n| (n.status, Reverse(n.strength)));
  Ok(networks)
}

async fn profile(conn: &Connection, path: OwnedObjectPath) -> Result<Settings> {
  let settings = ConnectionSettingsProxy::builder(conn)
    .path(path)?
    .cache_properties(CacheProperties::No)
    .build()
    .await?
    .get_settings()
    .await?;
  Ok(Settings::new(settings))
}

pub async fn saved_ssid(conn: &NmConnection<'_>) -> Result<Option<Vec<u8>>> {
  let settings = conn.get_settings().await?;
  let settings = Settings::new(settings);
  Ok(settings.wifi.and_then(|wifi| wifi.ssid))
}

pub struct AccessPointInfo {
  pub ssid: Vec<u8>,
  pub strength: u8,
  pub secured: bool,
  pub enterprise: bool,
}

pub async fn read_access_point(ap: &AccessPoint<'_>) -> Result<AccessPointInfo> {
  let security = ap.wpa_flags().await? | ap.rsn_flags().await?;
  let needs_credentials = ApSecurityFlags::KEY_MGMTPSK
    | ApSecurityFlags::KEY_MGMT_802_1X
    | ApSecurityFlags::KEY_MGMT_SAE
    | ApSecurityFlags::KEY_MGMT_EAP_SUITE_B_192;
  // WEP only sets the privacy flag, without any WPA/RSN key management
  let wep = security.is_empty() && ap.flags().await?.contains(ApFlags::PRIVACY);
  let secured = wep || security.intersects(needs_credentials);
  let enterprise = security
    .intersects(ApSecurityFlags::KEY_MGMT_802_1X | ApSecurityFlags::KEY_MGMT_EAP_SUITE_B_192);

  Ok(AccessPointInfo {
    ssid: ap.ssid().await?,
    strength: ap.strength().await?,
    secured,
    enterprise,
  })
}
