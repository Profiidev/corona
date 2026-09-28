use corona_network_manager as nm;
use corona_network_manager::{NetworkManager, NetworkManagerExt};
use gpui_kit::App;
use gpui_shell::HostModule;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
  host_fn::{Cx, Glob, Module},
  module::{Subscribe, Subscriptions, read},
};
use corona_macros::named;

#[derive(Serialize, TS)]
#[serde(rename_all = "snake_case")]
enum InterfaceType {
  Wired,
  Wireless,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "snake_case")]
enum DeviceState {
  Unmanaged,
  Unavailable,
  Disconnected,
  Prepare,
  Config,
  NeedAuth,
  IpConfig,
  IpCheck,
  Secondaries,
  Activated,
  Deactivating,
  Failed,
  Unknown,
}

#[derive(Serialize, TS)]
struct Ip {
  address: String,
  prefix: u32,
}

#[derive(Serialize, TS)]
struct Interface {
  name: String,
  kind: InterfaceType,
  state: DeviceState,
  ip: Option<Ip>,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "snake_case")]
enum Connectivity {
  None,
  Portal,
  Limited,
  Full,
  Unknown,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "snake_case")]
enum WifiStatus {
  Connected,
  NeedAuth,
  Connecting,
  Saved,
  New,
}

#[derive(Serialize, TS)]
struct WifiNetwork {
  ssid: String,
  strength: u8,
  secured: bool,
  /// 802.1X: join it with `connectEnterpriseWifi`.
  enterprise: bool,
  status: WifiStatus,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "snake_case")]
enum FailReason {
  NoSecrets,
  SsidNotFound,
  Other,
}

#[derive(Serialize, TS)]
struct WifiFailure {
  ssid: Option<String>,
  reason: FailReason,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "snake_case")]
enum VpnKind {
  Plugin,
  Wireguard,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "snake_case")]
enum VpnState {
  Activating,
  Activated,
  Deactivating,
  Deactivated,
  Unknown,
}

#[derive(Serialize, TS)]
struct Vpn {
  uuid: String,
  name: String,
  kind: VpnKind,
  state: VpnState,
}

/// The security of a hidden network, which cannot be read from an access point.
#[derive(Deserialize, TS)]
#[serde(rename_all = "snake_case")]
enum HiddenSecurity {
  Open,
  /// WPA/WPA2 personal, WPA3 transition mode included.
  Wpa,
  /// WPA3 personal only.
  Wpa3,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "snake_case")]
enum SecretKind {
  /// A WPA/WPA2/WPA3 personal password.
  Psk,
  /// An 802.1X login, like eduroam; asks for a username too when `identity` is null.
  Enterprise,
}

#[derive(Serialize, TS)]
struct SecretRequest {
  /// The SSID, or the profile name of a wired 802.1X connection.
  name: String,
  kind: SecretKind,
  /// The username stored in the 802.1X profile.
  identity: Option<String>,
  /// The previous secret was rejected.
  retry: bool,
}

impl From<&nm::Interface> for Interface {
  fn from(interface: &nm::Interface) -> Self {
    Self {
      name: interface.name.clone(),
      kind: match interface.kind {
        nm::InterfaceType::Wired => InterfaceType::Wired,
        nm::InterfaceType::Wireless => InterfaceType::Wireless,
      },
      state: match interface.state {
        nm::DeviceState::Unmanaged => DeviceState::Unmanaged,
        nm::DeviceState::Unavailable => DeviceState::Unavailable,
        nm::DeviceState::Disconnected => DeviceState::Disconnected,
        nm::DeviceState::Prepare => DeviceState::Prepare,
        nm::DeviceState::Config => DeviceState::Config,
        nm::DeviceState::NeedAuth => DeviceState::NeedAuth,
        nm::DeviceState::IpConfig => DeviceState::IpConfig,
        nm::DeviceState::IpCheck => DeviceState::IpCheck,
        nm::DeviceState::Secondaries => DeviceState::Secondaries,
        nm::DeviceState::Activated => DeviceState::Activated,
        nm::DeviceState::Deactivating => DeviceState::Deactivating,
        nm::DeviceState::Failed => DeviceState::Failed,
        nm::DeviceState::Unknown => DeviceState::Unknown,
      },
      ip: interface.ip.map(|ip| Ip {
        address: ip.address.to_string(),
        prefix: ip.prefix,
      }),
    }
  }
}

impl From<nm::NmConnectivityState> for Connectivity {
  fn from(state: nm::NmConnectivityState) -> Self {
    match state {
      nm::NmConnectivityState::None => Connectivity::None,
      nm::NmConnectivityState::Portal => Connectivity::Portal,
      nm::NmConnectivityState::Loss => Connectivity::Limited,
      nm::NmConnectivityState::Full => Connectivity::Full,
      nm::NmConnectivityState::Unknown => Connectivity::Unknown,
    }
  }
}

impl From<&nm::WifiNetwork> for WifiNetwork {
  fn from(network: &nm::WifiNetwork) -> Self {
    Self {
      ssid: network.ssid.clone(),
      strength: network.strength,
      secured: network.secured,
      enterprise: network.enterprise,
      status: match network.status {
        nm::WifiStatus::Connected => WifiStatus::Connected,
        nm::WifiStatus::NeedAuth => WifiStatus::NeedAuth,
        nm::WifiStatus::Connecting => WifiStatus::Connecting,
        nm::WifiStatus::Saved => WifiStatus::Saved,
        nm::WifiStatus::New => WifiStatus::New,
      },
    }
  }
}

impl From<&nm::WifiFailure> for WifiFailure {
  fn from(failure: &nm::WifiFailure) -> Self {
    Self {
      ssid: failure.ssid.clone(),
      reason: match failure.reason {
        nm::FailReason::NoSecrets => FailReason::NoSecrets,
        nm::FailReason::SsidNotFound => FailReason::SsidNotFound,
        nm::FailReason::Other(_) => FailReason::Other,
      },
    }
  }
}

impl From<&nm::Vpn> for Vpn {
  fn from(vpn: &nm::Vpn) -> Self {
    Self {
      uuid: vpn.uuid.clone(),
      name: vpn.name.clone(),
      kind: match vpn.kind {
        nm::VpnKind::Plugin => VpnKind::Plugin,
        nm::VpnKind::WireGuard => VpnKind::Wireguard,
      },
      state: match vpn.state {
        nm::ActiveConnectionState::Activating => VpnState::Activating,
        nm::ActiveConnectionState::Activated => VpnState::Activated,
        nm::ActiveConnectionState::Deactivating => VpnState::Deactivating,
        nm::ActiveConnectionState::Deactivated => VpnState::Deactivated,
        nm::ActiveConnectionState::Unknown => VpnState::Unknown,
      },
    }
  }
}

impl From<HiddenSecurity> for nm::HiddenSecurity {
  fn from(security: HiddenSecurity) -> Self {
    match security {
      HiddenSecurity::Open => nm::HiddenSecurity::Open,
      HiddenSecurity::Wpa => nm::HiddenSecurity::Wpa,
      HiddenSecurity::Wpa3 => nm::HiddenSecurity::Wpa3,
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Updates {
  Interfaces,
  PrimaryInterface,
  Connectivity,
  WifiSupported,
  WifiEnabled,
  PrimaryWifi,
  WifiNetworks,
  WifiFailure,
  SecretRequest,
  Vpns,
}

impl From<Updates> for super::Updates {
  fn from(value: Updates) -> Self {
    super::Updates::Network(value)
  }
}

pub fn module(reads: &Subscriptions, subs: &mut Vec<Subscribe>, cx: &mut App) -> HostModule {
  let network = cx.network_manager();

  Module::new("corona/network")
    .func(read(
      reads,
      subs,
      "listInterfaces",
      Updates::Interfaces,
      network.interfaces.clone(),
      |cx| {
        let interfaces = cx.network_manager().list_interfaces(cx);
        interfaces.iter().map(Interface::from).collect::<Vec<_>>()
      },
    ))
    .func(read(
      reads,
      subs,
      "primaryInterface",
      Updates::PrimaryInterface,
      network.primary_interface.clone(),
      |cx| {
        cx.network_manager()
          .primary_interface(cx)
          .map(Interface::from)
      },
    ))
    .func(read(
      reads,
      subs,
      "connectivity",
      Updates::Connectivity,
      network.connectivity.clone(),
      |cx| Connectivity::from(cx.network_manager().connectivity(cx)),
    ))
    .func(read(
      reads,
      subs,
      "wifiSupported",
      Updates::WifiSupported,
      network.wifi_supported.clone(),
      |cx| cx.network_manager().wifi_supported(cx),
    ))
    .func(read(
      reads,
      subs,
      "wifiEnabled",
      Updates::WifiEnabled,
      network.wifi_enabled.clone(),
      |cx| cx.network_manager().wifi_enabled(cx),
    ))
    .func(read(
      reads,
      subs,
      "primaryWifi",
      Updates::PrimaryWifi,
      network.primary_wifi.clone(),
      |cx| cx.network_manager().primary_wifi(cx).map(Interface::from),
    ))
    .func(read(
      reads,
      subs,
      "listWifiNetworks",
      Updates::WifiNetworks,
      network.wifi_networks.clone(),
      |cx| {
        let networks = cx.network_manager().list_wifi_networks(cx);
        networks.iter().map(WifiNetwork::from).collect::<Vec<_>>()
      },
    ))
    .func(read(
      reads,
      subs,
      "wifiFailure",
      Updates::WifiFailure,
      network.wifi_failure.clone(),
      |cx| cx.network_manager().wifi_failure(cx).map(WifiFailure::from),
    ))
    .func(read(
      reads,
      subs,
      "secretRequest",
      Updates::SecretRequest,
      network.secret_request.clone(),
      |cx| {
        let request = cx.network_manager().secret_request(cx);
        request.map(|request| SecretRequest {
          name: request.name.clone(),
          kind: match request.kind {
            nm::SecretKind::Psk => SecretKind::Psk,
            nm::SecretKind::Enterprise => SecretKind::Enterprise,
          },
          identity: request.identity.clone(),
          retry: request.retry,
        })
      },
    ))
    .func(named!(
      "answerSecret",
      /// Answers the pending secret request, or cancels it with a null password.
      /// `identity` is the 802.1X username, null keeps the profile's.
      |cx: &mut App, password: Option<String>, identity: Option<String>| {
        let secret = password.map(|password| nm::Secret { password, identity });
        cx.network_manager().clone().answer_secret(cx, secret)
      }
    ))
    .func(read(
      reads,
      subs,
      "listVpns",
      Updates::Vpns,
      network.vpns.clone(),
      |cx| {
        let vpns = cx.network_manager().list_vpns(cx);
        vpns.iter().map(Vpn::from).collect::<Vec<_>>()
      },
    ))
    .func(named!(
      "connectVpn",
      |nm: Glob<NetworkManager>, uuid: String| nm.connect_vpn(uuid)
    ))
    .func(named!(
      "disconnectVpn",
      |nm: Glob<NetworkManager>, uuid: String| nm.disconnect_vpn(uuid)
    ))
    .func(named!(
      "joinHiddenWifi",
      |cx: Cx,
       nm: Glob<NetworkManager>,
       ssid: String,
       security: HiddenSecurity,
       password: Option<String>| {
        nm.join_hidden_wifi(ssid, security.into(), password, &cx)
      }
    ))
    .func(named!(
      "setWifiEnabled",
      |nm: Glob<NetworkManager>, enabled: bool| nm.set_wifi_enabled(enabled)
    ))
    .func(named!("rescan", |cx: Cx, nm: Glob<NetworkManager>| nm.rescan(&cx)))
    .func(named!(
      "connectWifi",
      |cx: Cx, nm: Glob<NetworkManager>, ssid: String| nm.connect_wifi(ssid, &cx)
    ))
    .func(named!(
      "connectEnterpriseWifi",
      |cx: Cx, nm: Glob<NetworkManager>, config: nm::EnterpriseConfig| {
        nm.connect_enterprise_wifi(config, &cx)
      }
    ))
    .func(named!(
      "forgetWifi",
      |cx: Cx, nm: Glob<NetworkManager>, ssid: String| nm.forget_wifi(ssid, &cx)
    ))
    .func(named!(
      "connect",
      |cx: Cx, nm: Glob<NetworkManager>, name: String| nm.connect(&name, &cx)
    ))
    .func(named!(
      "disconnect",
      |cx: Cx, nm: Glob<NetworkManager>, name: String| nm.disconnect(&name, &cx)
    ))
    .into()
}
