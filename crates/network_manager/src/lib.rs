pub use agent::{Secret, SecretKind, SecretRequest};

use std::time::Duration;

use anyhow::{Context, Result};
use gpui_kit::{App, AppContext, Entity, Global};
use zbus::{Connection, zvariant::OwnedObjectPath};

use crate::listener::{agent_listener, listener, subscribe};

pub use crate::{
  actions::{EnterpriseConfig, HiddenSecurity, ScanResult},
  snapshot::{FailReason, WifiFailure, WifiNetwork, WifiStatus},
  state::{Interface, InterfaceType, Vpn, VpnKind},
};
pub use cosmic_dbus_networkmanager::interface::enums::{
  ActiveConnectionState, DeviceState, NmConnectivityState,
};

mod actions;
mod agent;
mod listener;
#[cfg(test)]
mod mock;
mod snapshot;
mod state;

const SCAN_TIMEOUT: Duration = Duration::from_secs(15);
const PORTAL_FALLBACK_URL: &str = "http://nmcheck.gnome.org/check_network_status.txt";

#[derive(Clone)]
pub struct NetworkManager {
  pub interfaces: Entity<Vec<Interface>>,
  pub primary_interface: Entity<Option<Interface>>,
  pub connectivity: Entity<NmConnectivityState>,
  pub connectivity_check: Entity<Option<String>>,
  pub wifi_supported: Entity<bool>,
  pub wifi_enabled: Entity<bool>,
  pub primary_wifi: Entity<Option<Interface>>,
  pub wifi_networks: Entity<Vec<WifiNetwork>>,
  pub wifi_failure: Entity<Option<WifiFailure>>,
  pub secret_request: Entity<Option<SecretRequest>>,
  pub vpns: Entity<Vec<Vpn>>,
  conn: Connection,
}

impl Global for NetworkManager {}

pub trait NetworkManagerExt {
  fn network_manager(&self) -> &NetworkManager;
}

impl NetworkManagerExt for App {
  fn network_manager(&self) -> &NetworkManager {
    self.global::<NetworkManager>()
  }
}

impl NetworkManager {
  pub fn list_interfaces<'c>(&self, cx: &'c App) -> &'c [Interface] {
    self.interfaces.read(cx)
  }

  pub fn primary_interface<'c>(&self, cx: &'c App) -> Option<&'c Interface> {
    self.primary_interface.read(cx).as_ref()
  }

  pub fn connectivity(&self, cx: &App) -> NmConnectivityState {
    *self.connectivity.read(cx)
  }

  pub fn connectivity_check<'c>(&self, cx: &'c App) -> Option<&'c str> {
    self.connectivity_check.read(cx).as_deref()
  }

  pub fn wifi_supported(&self, cx: &App) -> bool {
    *self.wifi_supported.read(cx)
  }

  pub fn list_vpns<'c>(&self, cx: &'c App) -> &'c [Vpn] {
    self.vpns.read(cx)
  }

  pub fn wifi_enabled(&self, cx: &App) -> bool {
    *self.wifi_enabled.read(cx)
  }

  pub fn primary_wifi<'c>(&self, cx: &'c App) -> Option<&'c Interface> {
    self.primary_wifi.read(cx).as_ref()
  }

  pub fn list_wifi_networks<'c>(&self, cx: &'c App) -> &'c [WifiNetwork] {
    self.wifi_networks.read(cx)
  }

  pub fn wifi_failure<'c>(&self, cx: &'c App) -> Option<&'c WifiFailure> {
    self.wifi_failure.read(cx).as_ref()
  }

  pub fn secret_request<'c>(&self, cx: &'c App) -> Option<&'c SecretRequest> {
    self.secret_request.read(cx).as_ref()
  }

  fn interface_path(&self, name: &str, cx: &App) -> Result<OwnedObjectPath> {
    self
      .list_interfaces(cx)
      .iter()
      .find(|i| i.name == name)
      .map(|i| i.path.clone())
      .with_context(|| format!("no interface {name}"))
  }

  fn wifi_path(&self, cx: &App) -> Result<OwnedObjectPath> {
    self
      .primary_wifi(cx)
      .map(|i| i.path.clone())
      .context("no usable wifi device")
  }

  // Actions read `cx` and clone the connection up front, so their futures are `'static` and can
  // run on the background executor.

  pub fn set_wifi_enabled(&self, enabled: bool) -> impl Future<Output = Result<()>> + use<> {
    let conn = self.conn.clone();
    async move { actions::set_wifi_enabled(&conn, enabled).await }
  }

  /// resolves once the scan finished, or with `TimedOut` after `SCAN_TIMEOUT`
  pub fn rescan(&self, cx: &App) -> impl Future<Output = Result<ScanResult>> + use<> {
    let (conn, device) = (self.conn.clone(), self.wifi_path(cx));
    let timeout = cx.background_executor().timer(SCAN_TIMEOUT);
    async move { actions::rescan(&conn, device?, timeout).await }
  }

  pub fn check_connectivity(&self) -> impl Future<Output = Result<NmConnectivityState>> + use<> {
    let conn = self.conn.clone();
    async move { actions::check_connectivity(&conn).await }
  }

  pub fn open_portal(&self, cx: &App) {
    let url = self
      .connectivity_check(cx)
      .filter(|uri| uri.starts_with("http://"))
      .unwrap_or(PORTAL_FALLBACK_URL);
    cx.open_url(url);
  }

  /// The SSID as listed broadcasts these bytes; one not in the list is taken as it is
  fn raw_ssid(&self, ssid: String, cx: &App) -> Vec<u8> {
    self
      .list_wifi_networks(cx)
      .iter()
      .find(|n| n.ssid == ssid)
      .map_or_else(|| ssid.into_bytes(), |n| n.raw_ssid.clone())
  }

  pub fn connect_wifi(&self, ssid: String, cx: &App) -> impl Future<Output = Result<()>> + use<> {
    let (conn, device, ssid) = (
      self.conn.clone(),
      self.wifi_path(cx),
      self.raw_ssid(ssid, cx),
    );
    async move { actions::connect_wifi(&conn, device?, ssid).await }
  }

  pub fn forget_wifi(&self, ssid: String, cx: &App) -> impl Future<Output = Result<()>> + use<> {
    let (conn, device, ssid) = (
      self.conn.clone(),
      self.wifi_path(cx),
      self.raw_ssid(ssid, cx),
    );
    async move { actions::forget_wifi(&conn, device?, ssid).await }
  }

  pub fn join_hidden_wifi(
    &self,
    ssid: String,
    security: HiddenSecurity,
    password: Option<String>,
    cx: &App,
  ) -> impl Future<Output = Result<()>> + use<> {
    let (conn, device) = (self.conn.clone(), self.wifi_path(cx));
    async move { actions::join_hidden_wifi(&conn, device?, ssid, security, password).await }
  }

  pub fn connect_vpn(&self, uuid: String) -> impl Future<Output = Result<()>> + use<> {
    let conn = self.conn.clone();
    async move { actions::connect_vpn(&conn, uuid).await }
  }

  pub fn disconnect_vpn(&self, uuid: String) -> impl Future<Output = Result<()>> + use<> {
    let conn = self.conn.clone();
    async move { actions::disconnect_vpn(&conn, uuid).await }
  }

  /// creates an 802.1X profile and connects the primary wifi device with it
  pub fn connect_enterprise_wifi(
    &self,
    config: EnterpriseConfig,
    cx: &App,
  ) -> impl Future<Output = Result<()>> + use<> {
    let (conn, device) = (self.conn.clone(), self.wifi_path(cx));
    async move { actions::connect_enterprise_wifi(&conn, device?, config).await }
  }

  /// activates the best saved profile of an interface
  pub fn connect(&self, interface: &str, cx: &App) -> impl Future<Output = Result<()>> + use<> {
    let (conn, device) = (self.conn.clone(), self.interface_path(interface, cx));
    async move { actions::connect_device(&conn, device?).await }
  }

  /// disconnects an interface, NM does not autoconnect it again until asked to
  pub fn disconnect(&self, interface: &str, cx: &App) -> impl Future<Output = Result<()>> + use<> {
    let (conn, device) = (self.conn.clone(), self.interface_path(interface, cx));
    async move { actions::disconnect_device(&conn, device?).await }
  }

  /// answer the pending secret request, None cancels it
  pub fn answer_secret(&self, cx: &mut App, secret: Option<Secret>) {
    let request = self.secret_request.update(cx, |request, cx| {
      cx.notify();
      request.take()
    });
    if let (Some(request), Some(secret)) = (request, secret) {
      let _ = request.reply.send(secret);
    }
  }
}

pub async fn init(cx: &mut App, conn: &Connection) -> Result<()> {
  let state = NetworkManager {
    interfaces: cx.new(|_| Vec::new()),
    primary_interface: cx.new(|_| None),
    connectivity: cx.new(|_| NmConnectivityState::Unknown),
    connectivity_check: cx.new(|_| None),
    wifi_supported: cx.new(|_| false),
    wifi_enabled: cx.new(|_| false),
    primary_wifi: cx.new(|_| None),
    wifi_networks: cx.new(|_| Vec::new()),
    wifi_failure: cx.new(|_| None),
    secret_request: cx.new(|_| None),
    vpns: cx.new(|_| Vec::new()),
    conn: conn.clone(),
  };

  // subscribe before the first snapshot so no change can slip in between
  let changes = subscribe(conn).await?;
  listener(cx, conn.clone(), changes, state.clone());
  // another agent (another corona, say) leaves NetworkManager to ask it for
  // passwords; everything else still works
  match agent::register(conn).await {
    Ok(events) => agent_listener(
      cx,
      events,
      state.secret_request.clone(),
      state.wifi_failure.clone(),
    ),
    Err(e) => tracing::warn!("Not asking for network passwords: {e:#}"),
  }
  cx.set_global(state);

  Ok(())
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
  use std::net::Ipv4Addr;

  use corona_utils::test_bus::{TestBus, settle, wait_until};
  use futures_lite::future::{block_on, poll_once};
  use gpui_kit::{self as gpui, TestAppContext};
  use zbus::zvariant::{OwnedValue, Value};

  use super::*;
  use crate::mock::{ETH, MockNm, NM, SETTINGS, SettingsMap, WLAN, World, ap};

  fn start(cx: &mut TestAppContext, world: World) -> (TestBus, MockNm) {
    cx.executor().allow_parking();
    let bus = TestBus::new();
    let nm = MockNm::start(&bus, world);
    let conn = block_on(bus.conn());
    cx.update(|cx| {
      cx.foreground_executor()
        .clone()
        .block_on(init(cx, &conn))
        .unwrap()
    });
    wait_until(cx, |cx| {
      cx.read(|cx| !cx.network_manager().list_interfaces(cx).is_empty())
    });
    (bus, nm)
  }

  fn nm(cx: &mut TestAppContext) -> NetworkManager {
    cx.read(|cx| cx.network_manager().clone())
  }

  fn networks(cx: &mut TestAppContext) -> Vec<(String, u8, bool, WifiStatus)> {
    cx.read(|cx| {
      cx.network_manager()
        .list_wifi_networks(cx)
        .iter()
        .map(|n| (n.ssid.clone(), n.strength, n.secured, n.status))
        .collect()
    })
  }

  #[gpui::test]
  fn reads_networkmanager(cx: &mut TestAppContext) {
    let (_bus, _nm) = start(cx, World::default());
    wait_until(cx, |cx| networks(cx).len() == 2);
    cx.read(|cx| {
      let nm = cx.network_manager();
      let interfaces = nm.list_interfaces(cx);
      assert_eq!(interfaces.len(), 2);
      let eth = &interfaces[0];
      assert_eq!(
        (eth.name.as_str(), eth.kind, eth.state),
        ("eth0", InterfaceType::Wired, DeviceState::Activated)
      );
      let ip = eth.ip.unwrap();
      assert_eq!((ip.address, ip.prefix), (Ipv4Addr::new(192, 168, 1, 5), 24));
      assert_eq!(interfaces[1].ip, None);
      assert_eq!(nm.primary_interface(cx).unwrap().name, "eth0");
      assert_eq!(nm.primary_wifi(cx).unwrap().name, "wlan0");
      assert!(nm.wifi_supported(cx) && nm.wifi_enabled(cx));
      assert_eq!(nm.connectivity(cx), NmConnectivityState::Full);
      assert_eq!(nm.connectivity_check(cx), Some("http://check.example/"));
      let vpns: Vec<_> = nm
        .list_vpns(cx)
        .iter()
        .map(|v| (v.name.as_str(), v.kind, v.state))
        .collect();
      assert_eq!(
        vpns,
        [
          (
            "Office VPN",
            VpnKind::Plugin,
            ActiveConnectionState::Deactivated
          ),
          ("wg0", VpnKind::WireGuard, ActiveConnectionState::Activating),
        ]
      );
      assert!(nm.wifi_failure(cx).is_none() && nm.secret_request(cx).is_none());
    });
    // the same SSID twice merges: strongest signal, saved first
    assert_eq!(
      networks(cx),
      [
        ("home".to_string(), 80, true, WifiStatus::Saved),
        ("cafe".to_string(), 70, false, WifiStatus::New),
      ]
    );
  }

  #[gpui::test]
  fn security_kinds(cx: &mut TestAppContext) {
    let mut world = World::default();
    let mut wep = ap(4, b"wep", 10, 0);
    wep.flags = 0x1;
    let mut corp = ap(5, b"corp", 20, 0x200);
    corp.wpa = 0x100;
    world.aps = vec![
      wep,
      corp,
      ap(6, b"owe", 30, 0x800),
      ap(7, b"sae", 40, 0x400),
      ap(8, b"", 99, 0),
    ];
    let (_bus, _nm) = start(cx, world);
    wait_until(cx, |cx| networks(cx).len() == 4);
    let secured: Vec<_> = networks(cx).into_iter().map(|n| (n.0, n.2)).collect();
    // hidden networks without an SSID are not listed
    assert_eq!(
      secured,
      [
        ("sae".to_string(), true),
        ("owe".to_string(), false),
        ("corp".to_string(), true),
        ("wep".to_string(), true)
      ]
    );
    cx.read(|cx| {
      let list = cx.network_manager().list_wifi_networks(cx);
      let enterprise: Vec<_> = list
        .iter()
        .filter(|n| n.enterprise)
        .map(|n| n.ssid.as_str())
        .collect();
      assert_eq!(enterprise, ["corp"]);
    });
  }

  #[gpui::test]
  fn the_connected_network(cx: &mut TestAppContext) {
    let mut world = World::default();
    world.wlan_state = 100;
    world.active_ap = format!("{NM}/AccessPoint/1");
    world.primary = "/".into();
    world.eth_state = 30;
    let (_bus, _nm) = start(cx, world);
    wait_until(cx, |cx| networks(cx).len() == 2);
    // the active AP's SSID is connected, even though another AP of it is stronger
    assert_eq!(
      networks(cx)[0],
      ("home".to_string(), 80, true, WifiStatus::Connected)
    );
    cx.read(|cx| {
      // no primary connection: an activated interface stands in
      assert_eq!(
        cx.network_manager().primary_interface(cx).unwrap().name,
        "wlan0"
      );
    });
  }

  #[gpui::test]
  fn connecting_states(cx: &mut TestAppContext) {
    for (state, status) in [
      (40, WifiStatus::Connecting),
      (70, WifiStatus::Connecting),
      (60, WifiStatus::NeedAuth),
    ] {
      let mut world = World::default();
      world.wlan_state = state;
      world.active_ap = format!("{NM}/AccessPoint/2");
      let (_bus, _nm) = start(cx, world);
      wait_until(cx, |cx| networks(cx).len() == 2);
      assert_eq!(networks(cx)[0], ("cafe".to_string(), 70, false, status));
      cx.update(|cx| cx.remove_global::<NetworkManager>());
    }
  }

  #[gpui::test]
  fn wired_preferred_without_a_primary(cx: &mut TestAppContext) {
    let mut world = World::default();
    world.primary = "/".into();
    world.wlan_state = 100;
    let (_bus, _nm) = start(cx, world);
    cx.read(|cx| {
      assert_eq!(
        cx.network_manager().primary_interface(cx).unwrap().name,
        "eth0"
      )
    });
  }

  #[gpui::test]
  fn without_wifi(cx: &mut TestAppContext) {
    let mut world = World::default();
    // the second device is wired too
    world.wlan_type = 1;
    world.check_enabled = false;
    let (_bus, _nm) = start(cx, world);
    settle(cx);
    let nm = nm(cx);
    cx.read(|cx| {
      assert!(!nm.wifi_supported(cx));
      assert!(nm.primary_wifi(cx).is_none() && nm.list_wifi_networks(cx).is_empty());
      assert_eq!(nm.connectivity_check(cx), None);
      assert_eq!(
        block_on(nm.connect_wifi("home".into(), cx))
          .unwrap_err()
          .to_string(),
        "no usable wifi device"
      );
      assert_eq!(
        block_on(nm.forget_wifi("home".into(), cx))
          .unwrap_err()
          .to_string(),
        "no usable wifi device"
      );
      assert_eq!(
        block_on(nm.rescan(cx)).unwrap_err().to_string(),
        "no usable wifi device"
      );
    });
  }

  #[gpui::test]
  fn unmanaged_wifi_is_unsupported(cx: &mut TestAppContext) {
    let mut world = World::default();
    world.wlan_state = 10;
    let (_bus, _nm) = start(cx, world);
    settle(cx);
    cx.read(|cx| {
      assert!(!cx.network_manager().wifi_supported(cx));
      assert!(cx.network_manager().primary_wifi(cx).is_none());
    });
  }

  #[gpui::test]
  fn follows_changes(cx: &mut TestAppContext) {
    let (_bus, mock) = start(cx, World::default());
    mock.world().connectivity = 2;
    mock.changed();
    wait_until(cx, |cx| {
      cx.read(|cx| cx.network_manager().connectivity(cx) == NmConnectivityState::Portal)
    });
  }

  #[gpui::test]
  fn portal_urls(cx: &mut TestAppContext) {
    let (_bus, mock) = start(cx, World::default());
    cx.update(|cx| cx.network_manager().open_portal(cx));
    assert_eq!(cx.opened_url().as_deref(), Some("http://check.example/"));
    // https cannot be intercepted by a portal, the fallback can
    mock.world().check_uri = "https://check.example/".into();
    mock.changed();
    wait_until(cx, |cx| {
      cx.read(|cx| cx.network_manager().connectivity_check(cx) == Some("https://check.example/"))
    });
    cx.update(|cx| cx.network_manager().open_portal(cx));
    assert_eq!(cx.opened_url().as_deref(), Some(PORTAL_FALLBACK_URL));
  }

  fn added(mock: &MockNm) -> SettingsMap {
    mock
      .world()
      .added
      .last()
      .cloned()
      .expect("a connection was added")
  }

  fn get(settings: &SettingsMap, setting: &str, key: &str) -> Option<OwnedValue> {
    settings.get(setting)?.get(key)?.try_clone().ok()
  }

  fn text(settings: &SettingsMap, setting: &str, key: &str) -> Option<String> {
    String::try_from(get(settings, setting, key)?).ok()
  }

  #[gpui::test]
  fn wifi_actions(cx: &mut TestAppContext) {
    let (_bus, mock) = start(cx, World::default());
    wait_until(cx, |cx| networks(cx).len() == 2);
    let nm = nm(cx);
    block_on(nm.set_wifi_enabled(false)).unwrap();
    // a saved network uses its profile
    block_on(cx.read(|cx| nm.connect_wifi("home".into(), cx))).unwrap();
    // a new one gets a profile, through its strongest access point
    block_on(cx.read(|cx| nm.connect_wifi("cafe".into(), cx))).unwrap();
    assert_eq!(
      block_on(cx.read(|cx| nm.connect_wifi("nowhere".into(), cx)))
        .unwrap_err()
        .to_string(),
      "nowhere is not in range"
    );
    block_on(cx.read(|cx| nm.forget_wifi("home".into(), cx))).unwrap();
    // nothing saved: nothing to forget
    block_on(cx.read(|cx| nm.forget_wifi("cafe".into(), cx))).unwrap();
    assert_eq!(
      mock
        .calls()
        .into_iter()
        .filter(|c| !c.starts_with("Register"))
        .collect::<Vec<_>>(),
      [
        "WirelessEnabled false".to_string(),
        format!("ActivateConnection {SETTINGS}/2 {WLAN} /"),
        format!("AddAndActivateConnection {WLAN} {NM}/AccessPoint/2"),
        format!("Delete {SETTINGS}/2"),
      ]
    );
    let cafe = added(&mock);
    assert_eq!(
      get(&cafe, "802-11-wireless", "ssid"),
      Some(Value::from(b"cafe".to_vec()).try_to_owned().unwrap())
    );
  }

  #[gpui::test]
  fn hidden_networks(cx: &mut TestAppContext) {
    let (_bus, mock) = start(cx, World::default());
    let nm = nm(cx);
    let join = |cx: &mut TestAppContext, security, password: Option<&str>| {
      let task =
        cx.read(|cx| nm.join_hidden_wifi("secret".into(), security, password.map(Into::into), cx));
      block_on(task).unwrap();
      added(&mock)
    };
    let open = join(cx, HiddenSecurity::Open, Some("ignored"));
    assert!(!open.contains_key("802-11-wireless-security"));
    assert_eq!(
      get(&open, "802-11-wireless", "hidden"),
      Some(Value::from(true).try_to_owned().unwrap())
    );
    let wpa = join(cx, HiddenSecurity::Wpa, Some("pw"));
    assert_eq!(
      text(&wpa, "802-11-wireless-security", "key-mgmt").as_deref(),
      Some("wpa-psk")
    );
    assert_eq!(
      text(&wpa, "802-11-wireless-security", "psk").as_deref(),
      Some("pw")
    );
    let sae = join(cx, HiddenSecurity::Wpa3, None);
    assert_eq!(
      text(&sae, "802-11-wireless-security", "key-mgmt").as_deref(),
      Some("sae")
    );
    assert_eq!(text(&sae, "802-11-wireless-security", "psk"), None);
    // no access point to aim at
    assert!(
      mock
        .calls()
        .iter()
        .all(|c| !c.starts_with("AddAndActivate") || c.ends_with(" /"))
    );
  }

  fn enterprise(ssid: &str, hidden: bool) -> EnterpriseConfig {
    serde_json_config(ssid, hidden)
  }

  fn serde_json_config(ssid: &str, hidden: bool) -> EnterpriseConfig {
    let json = format!(
      r#"{{"ssid": "{ssid}", "hidden": {hidden}, "eap": "peap", "phase2": "mschapv2", "identity": "me",
          "anonymous_identity": null, "password": "pw", "ca_cert": null, "domain": null,
          "client_cert": null, "private_key": null, "private_key_password": null}}"#
    );
    serde_json::from_str(&json).unwrap()
  }

  #[gpui::test]
  fn enterprise_networks(cx: &mut TestAppContext) {
    let (_bus, mock) = start(cx, World::default());
    let nm = nm(cx);
    block_on(cx.read(|cx| nm.connect_enterprise_wifi(enterprise("cafe", false), cx))).unwrap();
    block_on(cx.read(|cx| nm.connect_enterprise_wifi(enterprise("hidden-corp", true), cx)))
      .unwrap();
    let error =
      block_on(cx.read(|cx| nm.connect_enterprise_wifi(enterprise("far-away", false), cx)));
    assert_eq!(error.unwrap_err().to_string(), "far-away is not in range");
    let calls: Vec<_> = mock
      .calls()
      .into_iter()
      .filter(|c| c.starts_with("AddAndActivate"))
      .collect();
    assert_eq!(
      calls,
      [
        format!("AddAndActivateConnection {WLAN} {NM}/AccessPoint/2"),
        format!("AddAndActivateConnection {WLAN} /")
      ]
    );
    assert_eq!(
      text(&added(&mock), "802-1x", "password").as_deref(),
      Some("pw")
    );
  }

  #[gpui::test]
  fn vpn_and_device_actions(cx: &mut TestAppContext) {
    let (_bus, mock) = start(cx, World::default());
    let nm = nm(cx);
    block_on(nm.connect_vpn("uuid-Office VPN".into())).unwrap();
    assert!(block_on(nm.connect_vpn("uuid-unknown".into())).is_err());
    block_on(nm.disconnect_vpn("uuid-wg0".into())).unwrap();
    assert_eq!(
      block_on(nm.disconnect_vpn("uuid-Office VPN".into()))
        .unwrap_err()
        .to_string(),
      "uuid-Office VPN is not active"
    );
    block_on(cx.read(|cx| nm.connect("eth0", cx))).unwrap();
    block_on(cx.read(|cx| nm.disconnect("wlan0", cx))).unwrap();
    assert_eq!(
      block_on(cx.read(|cx| nm.connect("nope", cx)))
        .unwrap_err()
        .to_string(),
      "no interface nope"
    );
    assert_eq!(
      block_on(nm.check_connectivity()).unwrap(),
      NmConnectivityState::Portal
    );
    assert_eq!(
      mock
        .calls()
        .into_iter()
        .filter(|c| !c.starts_with("Register"))
        .collect::<Vec<_>>(),
      [
        format!("ActivateConnection {SETTINGS}/3 / /"),
        format!("DeactivateConnection {NM}/ActiveConnection/2"),
        format!("ActivateConnection / {ETH} /"),
        format!("Disconnect {WLAN}"),
        "CheckConnectivity".to_string(),
      ]
    );
  }

  #[gpui::test]
  fn rescans(cx: &mut TestAppContext) {
    let (_bus, mock) = start(cx, World::default());
    let nm = nm(cx);
    assert_eq!(
      block_on(cx.read(|cx| nm.rescan(cx))).unwrap(),
      ScanResult::Done
    );

    mock.world().scan_finishes = false;
    let scan = cx.read(|cx| nm.rescan(cx));
    let task = cx.executor().spawn(scan);
    wait_until(cx, |_| {
      mock.calls().iter().filter(|c| *c == "RequestScan").count() == 2
    });
    settle(cx);
    let mut task = Some(task);
    assert!(block_on(poll_once(task.as_mut().unwrap())).is_none());
    cx.executor().advance_clock(SCAN_TIMEOUT);
    let mut result = None;
    wait_until(cx, |_| {
      result = block_on(poll_once(task.as_mut().unwrap()));
      result.is_some()
    });
    assert_eq!(result.unwrap().unwrap(), ScanResult::TimedOut);
  }

  /// asks corona's agent for a secret like NetworkManager does
  fn get_secrets(
    mock: &MockNm,
    ssid: &[u8],
    connection_path: &str,
  ) -> std::thread::JoinHandle<zbus::Result<SettingsMap>> {
    let (agent, _) = mock.world().agent.clone().expect("an agent registered");
    let conn = mock.conn.clone();
    let settings: SettingsMap = std::collections::HashMap::from([(
      "802-11-wireless".to_string(),
      std::collections::HashMap::from([(
        "ssid".to_string(),
        Value::from(ssid.to_vec()).try_to_owned().unwrap(),
      )]),
    )]);
    let path = zbus::zvariant::OwnedObjectPath::try_from(connection_path).unwrap();
    std::thread::spawn(move || {
      block_on(async {
        let reply = conn
          .call_method(
            Some(agent.as_str()),
            "/org/freedesktop/NetworkManager/SecretAgent",
            Some("org.freedesktop.NetworkManager.SecretAgent"),
            "GetSecrets",
            &(
              settings,
              path,
              "802-11-wireless-security",
              Vec::<String>::new(),
              1u32,
            ),
          )
          .await?;
        reply.body().deserialize::<SettingsMap>()
      })
    })
  }

  fn cancel_secrets(mock: &MockNm, connection_path: &str) {
    let (agent, _) = mock.world().agent.clone().unwrap();
    let path = zbus::zvariant::OwnedObjectPath::try_from(connection_path).unwrap();
    block_on(mock.conn.call_method(
      Some(agent.as_str()),
      "/org/freedesktop/NetworkManager/SecretAgent",
      Some("org.freedesktop.NetworkManager.SecretAgent"),
      "CancelGetSecrets",
      &(path, "802-11-wireless-security"),
    ))
    .unwrap();
  }

  fn requested(cx: &mut TestAppContext) -> Option<String> {
    cx.read(|cx| {
      cx.network_manager()
        .secret_request(cx)
        .map(|r| r.name.clone())
    })
  }

  #[gpui::test]
  fn secret_agent(cx: &mut TestAppContext) {
    let (_bus, mock) = start(cx, World::default());
    assert_eq!(mock.world().agent.as_ref().unwrap().1, "io.corona.shell");

    let reply = get_secrets(&mock, b"home", "/s/2");
    wait_until(cx, |cx| requested(cx).as_deref() == Some("home"));
    cx.update(|cx| {
      let nm = cx.network_manager().clone();
      nm.answer_secret(
        cx,
        Some(Secret {
          password: "hunter22".into(),
          identity: None,
        }),
      );
    });
    assert!(requested(cx).is_none());
    let secrets = reply.join().unwrap().unwrap();
    let psk = secrets["802-11-wireless-security"]["psk"]
      .try_clone()
      .unwrap();
    assert_eq!(String::try_from(psk).unwrap(), "hunter22");

    // dismissing cancels
    let reply = get_secrets(&mock, b"home", "/s/2");
    wait_until(cx, |cx| requested(cx).is_some());
    cx.update(|cx| {
      let nm = cx.network_manager().clone();
      nm.answer_secret(cx, None);
    });
    let error = reply.join().unwrap().unwrap_err().to_string();
    assert!(error.contains("UserCanceled"), "{error}");
    // answering nothing pending is fine
    cx.update(|cx| {
      let nm = cx.network_manager().clone();
      nm.answer_secret(
        cx,
        Some(Secret {
          password: "x".into(),
          identity: None,
        }),
      );
    });

    // NetworkManager giving up closes the prompt
    let _reply = get_secrets(&mock, b"cafe", "/s/3");
    wait_until(cx, |cx| requested(cx).is_some());
    cancel_secrets(&mock, "/s/3");
    wait_until(cx, |cx| requested(cx).is_none());
  }

  #[gpui::test]
  fn late_cancel_keeps_the_newer_request(cx: &mut TestAppContext) {
    let (_bus, mock) = start(cx, World::default());
    let _old = get_secrets(&mock, b"home", "/s/2");
    wait_until(cx, |cx| requested(cx).as_deref() == Some("home"));
    let _new = get_secrets(&mock, b"cafe", "/s/3");
    wait_until(cx, |cx| requested(cx).as_deref() == Some("cafe"));
    cancel_secrets(&mock, "/s/2");
    settle(cx);
    assert_eq!(requested(cx).as_deref(), Some("cafe"));
  }

  fn failure(cx: &mut TestAppContext) -> Option<WifiFailure> {
    cx.read(|cx| cx.network_manager().wifi_failure(cx).cloned())
  }

  #[gpui::test]
  fn wifi_failures(cx: &mut TestAppContext) {
    let mut world = World::default();
    world.wlan_state = 40;
    world.active_ap = format!("{NM}/AccessPoint/2");
    let (_bus, mock) = start(cx, world);
    wait_until(cx, |cx| {
      networks(cx)
        .first()
        .is_some_and(|n| n.3 == WifiStatus::Connecting)
    });

    mock.world().wlan_state = 120;
    mock.device_state(WLAN, 120, 40, 7);
    mock.changed();
    wait_until(cx, |cx| failure(cx).is_some());
    assert_eq!(
      failure(cx),
      Some(WifiFailure {
        ssid: Some("cafe".into()),
        reason: FailReason::NoSecrets
      })
    );

    // disconnecting after the failure keeps it, the wired device's states are not wifi's
    mock.device_state(WLAN, 30, 120, 0);
    mock.device_state(ETH, 40, 30, 0);
    mock.changed();
    settle(cx);
    assert!(failure(cx).is_some());

    // trying again clears it
    mock.device_state(WLAN, 40, 30, 0);
    mock.changed();
    wait_until(cx, |cx| failure(cx).is_none());

    // a password prompt clears it too
    mock.device_state(WLAN, 120, 40, 53);
    mock.changed();
    wait_until(cx, |cx| {
      failure(cx).is_some_and(|f| f.reason == FailReason::SsidNotFound)
    });
    let _reply = get_secrets(&mock, b"cafe", "/s/3");
    wait_until(cx, |cx| failure(cx).is_none());
  }

  #[gpui::test]
  fn non_utf8_ssids_connect(cx: &mut TestAppContext) {
    let mut world = World::default();
    world.aps.push(ap(4, b"caf\xe9", 50, 0));
    let (_bus, mock) = start(cx, world);
    wait_until(cx, |cx| networks(cx).len() == 3);
    let ssid = networks(cx)
      .into_iter()
      .find(|n| n.0.starts_with("caf\u{fffd}"))
      .unwrap()
      .0;
    let nm = nm(cx);
    block_on(cx.read(|cx| nm.connect_wifi(ssid, cx))).unwrap();
    assert!(mock.calls().iter().any(|c| c.ends_with("AccessPoint/4")));
  }

  #[gpui::test]
  fn network_order_is_stable(cx: &mut TestAppContext) {
    let mut world = World::default();
    world.aps = vec![ap(1, b"b", 50, 0), ap(2, b"a", 50, 0)];
    let (_bus, _mock) = start(cx, world);
    wait_until(cx, |cx| networks(cx).len() == 2);
    let order: Vec<_> = networks(cx).into_iter().map(|n| n.0).collect();
    assert_eq!(order, ["a", "b"]);
  }

  #[gpui::test]
  fn networkmanager_restarts_are_noticed(cx: &mut TestAppContext) {
    let (bus, mock) = start(cx, World::default());
    assert!(mock.world().agent.is_some());
    drop(mock);
    let mut world = World::default();
    world.connectivity = 1;
    let restarted = MockNm::start(&bus, world);
    wait_until(cx, |cx| {
      cx.read(|cx| cx.network_manager().connectivity(cx) == NmConnectivityState::None)
    });
    // The secret agent is not re-registered after daemon restart
    assert!(restarted.world().agent.is_none());
  }

  #[gpui::test]
  fn transient_primary_connection_deactivation_crashes_snapshot(_cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let mut world = World::default();
    world.primary = format!("{NM}/ActiveConnection/999");
    let _mock = MockNm::start(&bus, world);
    let conn = block_on(bus.conn());
    let res = block_on(snapshot::snapshot(&conn));
    assert!(res.is_err());
  }

  #[gpui::test]
  fn wireless_without_active_ap(cx: &mut TestAppContext) {
    let mut world = World::default();
    world.active_ap = "/".into();
    let (_bus, _mock) = start(cx, world);
    wait_until(cx, |cx| networks(cx).len() == 2);
    for (_, _, _, status) in networks(cx) {
      assert_ne!(status, WifiStatus::Connected);
      assert_ne!(status, WifiStatus::Connecting);
    }
  }

  #[gpui::test]
  fn merging_disparate_security_flags_on_identical_ssid(cx: &mut TestAppContext) {
    let mut world = World::default();
    let mut enterprise_ap = ap(2, b"hybrid", 80, 0x200);
    enterprise_ap.wpa = 0x100;
    world.aps = vec![ap(1, b"hybrid", 30, 0), enterprise_ap];
    let (_bus, _mock) = start(cx, world);
    wait_until(cx, |cx| networks(cx).len() == 1);
    let (strength, secured, enterprise) = cx.read(|cx| {
      let net = cx
        .network_manager()
        .list_wifi_networks(cx)
        .iter()
        .find(|n| n.ssid == "hybrid")
        .unwrap();
      (net.strength, net.secured, net.enterprise)
    });
    assert_eq!(strength, 80);
    assert!(secured);
    assert!(enterprise);
  }

  #[gpui::test]
  fn scan_rate_limiting_error_propagates(cx: &mut TestAppContext) {
    let mut world = World::default();
    world.scan_fails = true;
    let (_bus, mock) = start(cx, world);
    let nm = nm(cx);
    let scan_task = cx.read(|cx| nm.rescan(cx));
    let result = block_on(scan_task);
    assert!(result.is_err());
    assert!(mock.calls().contains(&"RequestScan".to_string()));
  }

  #[gpui::test]
  fn connect_wifi_fails_if_not_in_range_even_if_saved(cx: &mut TestAppContext) {
    let mut world = World::default();
    world.aps.retain(|a| a.ssid != b"home");
    let (_bus, mock) = start(cx, world);
    let nm = nm(cx);
    let err = block_on(cx.read(|cx| nm.connect_wifi("home".into(), cx))).unwrap_err();
    assert!(err.to_string().contains("home is not in range"));
    assert!(
      !mock
        .calls()
        .iter()
        .any(|c| c.starts_with("ActivateConnection"))
    );
  }

  #[gpui::test]
  fn forget_wifi_when_no_connections_available(cx: &mut TestAppContext) {
    let mut world = World::default();
    world.available = Vec::new();
    let (_bus, mock) = start(cx, world);
    let nm = nm(cx);
    block_on(cx.read(|cx| nm.forget_wifi("home".into(), cx))).unwrap();
    assert!(!mock.calls().iter().any(|c| c.starts_with("Delete")));
  }

  #[gpui::test]
  fn non_utf8_ssid_collision_resolves_to_first(cx: &mut TestAppContext) {
    let mut world = World::default();
    world.aps = vec![ap(4, b"caf\xe9", 60, 0), ap(5, b"caf\xfa", 40, 0)];
    let (_bus, mock) = start(cx, world);
    wait_until(cx, |cx| networks(cx).len() == 2);
    let nm = nm(cx);
    let lossy_name = "caf\u{fffd}".to_string();
    block_on(cx.read(|cx| nm.connect_wifi(lossy_name, cx))).unwrap();
    assert!(mock.calls().iter().any(|c| c.ends_with("AccessPoint/4")));
    assert!(!mock.calls().iter().any(|c| c.ends_with("AccessPoint/5")));
  }
}
