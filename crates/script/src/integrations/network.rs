use corona_network_manager as nm;
use corona_network_manager::NetworkManagerExt;
use corona_js::{
  Cx, Glob, Json, ModuleBuilder, Opt, PluginRef, ScriptModule, Throws, host_module, json,
};
use gpui_kit::App;
use serde::{Deserialize, Serialize};
use specta::{Type, specta};

#[derive(Serialize, Type)]
#[serde(rename_all = "snake_case")]
enum InterfaceType {
  Wired,
  Wireless,
}

#[derive(Serialize, Type)]
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

#[derive(Serialize, Type)]
struct Ip {
  address: String,
  prefix: u32,
}

#[derive(Serialize, Type)]
struct Interface {
  name: String,
  kind: InterfaceType,
  state: DeviceState,
  ip: Option<Ip>,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "snake_case")]
enum Connectivity {
  None,
  Portal,
  Limited,
  Full,
  Unknown,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "snake_case")]
enum ScanResult {
  Done,
  TimedOut,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "snake_case")]
enum WifiStatus {
  Connected,
  NeedAuth,
  Connecting,
  Saved,
  New,
}

#[derive(Serialize, Type)]
struct WifiNetwork {
  ssid: String,
  strength: u8,
  secured: bool,
  /// 802.1X: join it with `connectEnterpriseWifi`.
  enterprise: bool,
  status: WifiStatus,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "snake_case")]
enum FailReason {
  NoSecrets,
  SsidNotFound,
  Other,
}

#[derive(Serialize, Type)]
struct WifiFailure {
  ssid: Option<String>,
  reason: FailReason,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "snake_case")]
enum VpnKind {
  Plugin,
  Wireguard,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "snake_case")]
enum VpnState {
  Activating,
  Activated,
  Deactivating,
  Deactivated,
  Unknown,
}

#[derive(Serialize, Type)]
struct Vpn {
  uuid: String,
  name: String,
  kind: VpnKind,
  state: VpnState,
}

/// The security of a hidden network, which cannot be read from an access point.
#[derive(Deserialize, Type)]
#[serde(rename_all = "snake_case")]
enum HiddenSecurity {
  Open,
  /// WPA/WPA2 personal, WPA3 transition mode included.
  Wpa,
  /// WPA3 personal only.
  Wpa3,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "snake_case")]
enum SecretKind {
  /// A WPA/WPA2/WPA3 personal password.
  Psk,
  /// An 802.1X login, like eduroam; asks for a username too when `identity` is null.
  Enterprise,
}

#[derive(Serialize, Type)]
struct SecretRequest {
  /// The SSID, or the profile name of a wired 802.1X connection.
  name: String,
  kind: SecretKind,
  /// The username stored in the 802.1X profile.
  identity: Option<String>,
  /// The previous secret was rejected.
  retry: bool,
}

#[derive(Serialize, Type)]
struct Hotspot {
  /// Up, not still activating.
  active: bool,
  ssid: String,
  band: nm::HotspotBand,
}

/// The hotspot's settings without its password, which plugins never see.
#[derive(Serialize, Type)]
struct HotspotSettings {
  ssid: String,
  /// It has a password, which `startHotspot` needs again to keep it.
  secured: bool,
  band: nm::HotspotBand,
  wpa3: bool,
  /// Minutes without clients it turns off after, null: never.
  auto_off: Option<u32>,
  /// Keeps the clients from reaching each other.
  isolate: bool,
}

impl From<nm::HotspotConfig> for HotspotSettings {
  fn from(config: nm::HotspotConfig) -> Self {
    Self {
      ssid: config.ssid,
      secured: config.password.is_some(),
      band: config.band,
      wpa3: config.wpa3,
      auto_off: config.auto_off,
      isolate: config.isolate,
    }
  }
}

impl From<&nm::HotspotInfo> for Hotspot {
  fn from(hotspot: &nm::HotspotInfo) -> Self {
    Self {
      active: hotspot.active,
      ssid: hotspot.ssid.clone(),
      band: hotspot.band,
    }
  }
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

type Nm = Glob<nm::NetworkManager>;

#[specta]
async fn check_connectivity(nm: Nm) -> Throws<Json<Connectivity>> {
  nm.check_connectivity().await.map(|c| Json(c.into())).into()
}

#[specta]
fn open_portal(cx: Cx) {
  cx.network_manager().open_portal(&cx)
}

/// Answers the pending secret request, or cancels it with a null password. `identity` is
/// the 802.1X username, null keeps the profile's.
#[specta]
fn answer_secret(mut cx: Cx, password: Option<String>, identity: Opt<String>) {
  let identity = identity.0;
  let secret = password.map(|password| nm::Secret { password, identity });
  cx.network_manager().clone().answer_secret(&mut cx, secret)
}

#[specta]
async fn connect_vpn(nm: Nm, uuid: String) -> Throws<()> {
  nm.connect_vpn(uuid).await.into()
}

#[specta]
async fn disconnect_vpn(nm: Nm, uuid: String) -> Throws<()> {
  nm.disconnect_vpn(uuid).await.into()
}

#[specta]
async fn join_hidden_wifi(
  cx: Cx,
  nm: Nm,
  ssid: String,
  security: Json<HiddenSecurity>,
  password: Opt<String>,
) -> Throws<()> {
  let join = nm.join_hidden_wifi(ssid, security.0.into(), password.0, &cx);
  join.await.into()
}

#[specta]
async fn set_wifi_enabled(nm: Nm, enabled: bool) -> Throws<()> {
  nm.set_wifi_enabled(enabled).await.into()
}

#[specta]
async fn rescan(cx: Cx, nm: Nm) -> Throws<Json<ScanResult>> {
  let scan = nm.rescan(&cx);
  let scanned = scan.await.map(|result| match result {
    nm::ScanResult::Done => ScanResult::Done,
    nm::ScanResult::TimedOut => ScanResult::TimedOut,
  });
  scanned.map(Json).into()
}

#[specta]
async fn connect_wifi(cx: Cx, nm: Nm, ssid: String) -> Throws<()> {
  let connect = nm.connect_wifi(ssid, &cx);
  connect.await.into()
}

#[specta]
async fn connect_enterprise_wifi(cx: Cx, nm: Nm, config: Json<nm::EnterpriseConfig>) -> Throws<()> {
  let connect = nm.connect_enterprise_wifi(config.0, &cx);
  connect.await.into()
}

#[specta]
async fn forget_wifi(cx: Cx, nm: Nm, ssid: String) -> Throws<()> {
  let forget = nm.forget_wifi(ssid, &cx);
  forget.await.into()
}

#[specta]
async fn connect(cx: Cx, nm: Nm, name: String) -> Throws<()> {
  let connect = nm.connect(&name, &cx);
  connect.await.into()
}

/// Turns the wifi device into an access point; a null password makes it open.
#[specta]
async fn start_hotspot(cx: Cx, nm: Nm, config: Json<nm::HotspotConfig>) -> Throws<()> {
  let start = nm.start_hotspot(config.0, &cx);
  start.await.into()
}

#[specta]
async fn stop_hotspot(cx: Cx, nm: Nm) -> Throws<()> {
  let stop = nm.stop_hotspot(&cx);
  stop.await.into()
}

/// The running hotspot's settings, else the saved ones, else null. Never its password.
#[specta]
async fn hotspot_config(cx: Cx, nm: Nm) -> Throws<Json<Option<HotspotSettings>>> {
  let config = nm.hotspot_config(&cx);
  let config = config.await.map(|config| config.map(HotspotSettings::from));
  config.map(Json).into()
}

#[specta]
async fn disconnect(cx: Cx, nm: Nm, name: String) -> Throws<()> {
  let disconnect = nm.disconnect(&name, &cx);
  disconnect.await.into()
}

/// `corona/network`
pub struct NetworkModule;

impl ScriptModule for NetworkModule {
  const NAME: &str = "network";
  const DOC: &str = "Network interfaces, Wi-Fi, VPNs and the hotspot.";

  fn module(_: &PluginRef, m: &mut ModuleBuilder, _: &mut App) {
    m.watch(
      "listInterfaces",
      "",
      |cx| cx.network_manager().interfaces.clone(),
      json(|cx| {
        let interfaces = cx.network_manager().list_interfaces(cx);
        interfaces.iter().map(Interface::from).collect::<Vec<_>>()
      }),
    );
    m.watch(
      "primaryInterface",
      "",
      |cx| cx.network_manager().primary_interface.clone(),
      json(|cx| {
        cx.network_manager()
          .primary_interface(cx)
          .map(Interface::from)
      }),
    );
    m.watch(
      "connectivity",
      "",
      |cx| cx.network_manager().connectivity.clone(),
      json(|cx| Connectivity::from(cx.network_manager().connectivity(cx))),
    );
    m.watch(
      "connectivityCheckEnabled",
      "",
      |cx| cx.network_manager().connectivity_check.clone(),
      json(|cx| cx.network_manager().connectivity_check(cx).is_some()),
    );
    m.watch(
      "wifiSupported",
      "",
      |cx| cx.network_manager().wifi_supported.clone(),
      json(|cx| cx.network_manager().wifi_supported(cx)),
    );
    m.watch(
      "wifiEnabled",
      "",
      |cx| cx.network_manager().wifi_enabled.clone(),
      json(|cx| cx.network_manager().wifi_enabled(cx)),
    );
    m.watch(
      "primaryWifi",
      "",
      |cx| cx.network_manager().primary_wifi.clone(),
      json(|cx| cx.network_manager().primary_wifi(cx).map(Interface::from)),
    );
    m.watch(
      "listWifiNetworks",
      "",
      |cx| cx.network_manager().wifi_networks.clone(),
      json(|cx| {
        let networks = cx.network_manager().list_wifi_networks(cx);
        networks.iter().map(WifiNetwork::from).collect::<Vec<_>>()
      }),
    );
    m.watch(
      "wifiFailure",
      "",
      |cx| cx.network_manager().wifi_failure.clone(),
      json(|cx| cx.network_manager().wifi_failure(cx).map(WifiFailure::from)),
    );
    m.watch(
      "secretRequest",
      "",
      |cx| cx.network_manager().secret_request.clone(),
      json(|cx| {
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
      }),
    );
    m.watch(
      "listVpns",
      "",
      |cx| cx.network_manager().vpns.clone(),
      json(|cx| {
        let vpns = cx.network_manager().list_vpns(cx);
        vpns.iter().map(Vpn::from).collect::<Vec<_>>()
      }),
    );
    m.watch(
      "hotspot",
      "",
      |cx| cx.network_manager().hotspot.clone(),
      json(|cx| cx.network_manager().hotspot(cx).map(Hotspot::from)),
    );
    host_module!(m, {
      sync: [open_portal, answer_secret],
      async: [
        check_connectivity,
        connect_vpn,
        disconnect_vpn,
        join_hidden_wifi,
        set_wifi_enabled,
        rescan,
        connect_wifi,
        connect_enterprise_wifi,
        forget_wifi,
        connect,
        start_hotspot,
        stop_hotspot,
        hotspot_config,
        disconnect,
      ],
    });
  }
}

#[cfg(test)]
mod tests {
  use corona_network_manager as nm;

  use super::*;

  fn json(value: impl Serialize) -> serde_json::Value {
    serde_json::to_value(value).unwrap()
  }

  fn interface(kind: nm::InterfaceType, state: nm::DeviceState) -> nm::Interface {
    nm::Interface {
      path: Default::default(),
      name: "wlan0".into(),
      ip: None,
      kind,
      state,
    }
  }

  #[test]
  fn interfaces() {
    let states = [
      (nm::DeviceState::Unmanaged, "unmanaged"),
      (nm::DeviceState::Unavailable, "unavailable"),
      (nm::DeviceState::Disconnected, "disconnected"),
      (nm::DeviceState::Prepare, "prepare"),
      (nm::DeviceState::Config, "config"),
      (nm::DeviceState::NeedAuth, "need_auth"),
      (nm::DeviceState::IpConfig, "ip_config"),
      (nm::DeviceState::IpCheck, "ip_check"),
      (nm::DeviceState::Secondaries, "secondaries"),
      (nm::DeviceState::Activated, "activated"),
      (nm::DeviceState::Deactivating, "deactivating"),
      (nm::DeviceState::Failed, "failed"),
      (nm::DeviceState::Unknown, "unknown"),
    ];
    for (state, name) in states {
      let json = json(Interface::from(&interface(nm::InterfaceType::Wired, state)));
      assert_eq!(json["state"], name);
      assert_eq!(json["kind"], "wired");
      assert_eq!(json["name"], "wlan0");
      assert!(json["ip"].is_null());
    }
    let wireless = interface(nm::InterfaceType::Wireless, nm::DeviceState::Activated);
    assert_eq!(json(Interface::from(&wireless))["kind"], "wireless");
  }

  #[test]
  fn connectivity() {
    let all = [
      (nm::NmConnectivityState::None, "none"),
      (nm::NmConnectivityState::Portal, "portal"),
      (nm::NmConnectivityState::Loss, "limited"),
      (nm::NmConnectivityState::Full, "full"),
      (nm::NmConnectivityState::Unknown, "unknown"),
    ];
    for (state, name) in all {
      assert_eq!(json(Connectivity::from(state)), name);
    }
  }

  #[test]
  fn wifi_networks() {
    let all = [
      (nm::WifiStatus::Connected, "connected"),
      (nm::WifiStatus::NeedAuth, "need_auth"),
      (nm::WifiStatus::Connecting, "connecting"),
      (nm::WifiStatus::Saved, "saved"),
      (nm::WifiStatus::New, "new"),
    ];
    for (status, name) in all {
      let network = nm::WifiNetwork {
        ssid: "home".into(),
        raw_ssid: b"home".to_vec(),
        strength: 70,
        secured: true,
        enterprise: false,
        status,
      };
      let json = json(WifiNetwork::from(&network));
      assert_eq!(json["status"], name);
      assert_eq!(json["ssid"], "home");
      assert_eq!(json["strength"], 70);
      assert_eq!(json["secured"], true);
      assert_eq!(json["enterprise"], false);
      // the raw SSID stays in the shell
      assert!(json.get("raw_ssid").is_none());
    }
  }

  #[test]
  fn wifi_failures() {
    let all = [
      (nm::FailReason::NoSecrets, "no_secrets"),
      (nm::FailReason::SsidNotFound, "ssid_not_found"),
      (nm::FailReason::Other(0), "other"),
      (nm::FailReason::Other(53), "other"),
    ];
    for (reason, name) in all {
      let failure = nm::WifiFailure {
        ssid: Some("home".into()),
        reason,
      };
      let json = json(WifiFailure::from(&failure));
      assert_eq!(json["reason"], name);
      assert_eq!(json["ssid"], "home");
    }
  }

  #[test]
  fn vpns() {
    let states = [
      (nm::ActiveConnectionState::Activating, "activating"),
      (nm::ActiveConnectionState::Activated, "activated"),
      (nm::ActiveConnectionState::Deactivating, "deactivating"),
      (nm::ActiveConnectionState::Deactivated, "deactivated"),
      (nm::ActiveConnectionState::Unknown, "unknown"),
    ];
    for (kind, kind_name) in [
      (nm::VpnKind::Plugin, "plugin"),
      (nm::VpnKind::WireGuard, "wireguard"),
    ] {
      for (state, name) in states {
        let vpn = nm::Vpn {
          uuid: "u".into(),
          name: "work".into(),
          kind,
          state,
        };
        let json = json(Vpn::from(&vpn));
        assert_eq!(json["kind"], kind_name);
        assert_eq!(json["state"], name);
        assert_eq!(json["uuid"], "u");
      }
    }
  }

  #[test]
  fn hotspots() {
    let hotspot = nm::HotspotInfo {
      active: true,
      ssid: "corona".into(),
      band: nm::HotspotBand::A,
      connection_path: Default::default(),
      interface: "wlan0".into(),
      auto_off: Some(10),
      corona: true,
      channel: None,
    };
    let json = json(Hotspot::from(&hotspot));
    assert_eq!(json["band"], "a");
    assert_eq!(json["active"], true);
    assert!(json.get("connection_path").is_none());
    let config: nm::HotspotConfig =
      serde_json::from_str(r#"{"ssid": "corona", "password": null, "band": "bg"}"#).unwrap();
    assert_eq!(config.band, nm::HotspotBand::Bg);
    // older scripts leave them out
    assert!(!config.wpa3);
    assert_eq!(config.auto_off, Some(10));
    assert!(config.isolate);
    let secured = nm::HotspotConfig {
      password: Some("hunter22".into()),
      ..config
    };
    let settings = super::tests::json(HotspotSettings::from(secured));
    assert!(settings.get("password").is_none());
    assert_eq!(settings["secured"], true);
  }

  #[test]
  fn hidden_security() {
    let all = [
      ("open", nm::HiddenSecurity::Open),
      ("wpa", nm::HiddenSecurity::Wpa),
      ("wpa3", nm::HiddenSecurity::Wpa3),
    ];
    for (name, security) in all {
      let parsed: HiddenSecurity = serde_json::from_value(name.into()).unwrap();
      assert_eq!(nm::HiddenSecurity::from(parsed), security);
    }
    assert!(serde_json::from_value::<HiddenSecurity>("wep".into()).is_err());
  }
}
