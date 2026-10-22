pub use agent::{Secret, SecretKind, SecretRequest};

use std::{
  sync::{Arc, Mutex},
  time::Duration,
};

use anyhow::{Context, Result};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{App, AppContext, AsyncApp, BackgroundExecutor, Entity, Global, Task};
use zbus::{Connection, zvariant::OwnedObjectPath};

use crate::listener::{agent_listener, listener, subscribe};

pub use crate::{
  actions::{EnterpriseConfig, HiddenSecurity, HotspotConfig, ScanResult},
  snapshot::{FailReason, WifiFailure, WifiNetwork, WifiStatus},
  state::{HotspotBand, HotspotClient, HotspotInfo, Interface, InterfaceType, Vpn, VpnKind},
};
pub use cosmic_dbus_networkmanager::interface::enums::{
  ActiveConnectionState, DeviceState, NmConnectivityState,
};

mod actions;
mod agent;
mod forwarding;
mod listener;
#[cfg(test)]
mod mock;
pub mod nl80211;
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
  pub hotspot: Entity<Option<HotspotInfo>>,
  /// the primary wifi device's radio runs a hotspot beside its connection
  pub hotspot_beside_wifi: Entity<bool>,
  /// the firewall rules corona let the hotspot's clients through with, None: none
  hotspot_rules: Arc<Mutex<Option<HotspotRules>>>,
  /// one root call at a time, so a late one cannot undo a later one
  root: RootLock,
  conn: Connection,
}

#[derive(Clone, Debug, PartialEq)]
struct HotspotRules {
  interface: String,
  uplink: Option<String>,
  /// the uplink is closed to routing elsewhere: it did not route before the hotspot
  lock_uplink: bool,
}

impl HotspotRules {
  /// `uplink` unless it is the hotspot's own interface, which is no way out
  fn new(interface: &str, uplink: Option<&str>) -> Self {
    let uplink = uplink.filter(|u| *u != interface).map(str::to_owned);
    Self {
      lock_uplink: uplink.as_deref().is_some_and(forwarding::lock_needed),
      uplink,
      interface: interface.to_owned(),
    }
  }

  fn allow(&self) -> impl Future<Output = Result<()>> + use<> {
    let rules = self.clone();
    async move {
      actions::allow_hotspot(&rules.interface, rules.uplink.as_deref(), rules.lock_uplink).await
    }
  }
}

/// a one token semaphore
#[derive(Clone)]
struct RootLock(flume::Sender<()>, flume::Receiver<()>);

impl RootLock {
  fn new() -> Self {
    let (token, wait) = flume::bounded(1);
    token.send(()).unwrap();
    Self(token, wait)
  }

  async fn run<T>(&self, f: impl Future<Output = T>) -> T {
    self.1.recv_async().await.ok();
    let out = f.await;
    self.0.send(()).ok();
    out
  }
}

/// how long the wifi connection has to stay on another channel before the hotspot follows it
const ROAM_DEBOUNCE: Duration = Duration::from_secs(5);
/// failed client counts in a row auto-off gives up after
const AUTO_OFF_FAILURES: u32 = 5;

/// how often a hotspot that turns itself off counts its clients
const AUTO_OFF_CHECK: Duration = Duration::from_secs(30);

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

  pub fn hotspot<'c>(&self, cx: &'c App) -> Option<&'c HotspotInfo> {
    self.hotspot.read(cx).as_ref()
  }

  pub fn hotspot_beside_wifi(&self, cx: &App) -> bool {
    *self.hotspot_beside_wifi.read(cx)
  }

  /// a usable wifi device besides the primary one, like corona's own access point interface or
  /// a second adapter: the hotspot runs there and the primary stays connected
  pub fn spare_wifi<'c>(&self, cx: &'c App) -> Option<&'c Interface> {
    let primary = self.primary_wifi(cx).map(|i| &i.path);
    self
      .list_interfaces(cx)
      .iter()
      .find(|i| i.kind == InterfaceType::Wireless && Some(&i.path) != primary && i.usable())
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

  /// runs an access point through the Corona Hotspot profile: on a spare wifi device or corona's
  /// own interface, else on an interface added beside the primary one where its radio allows,
  /// else on the primary one, whose connection drops meanwhile. Asks for root to let the clients
  /// through the firewall to the uplink, along with adding the interface
  pub fn start_hotspot(
    &self,
    config: HotspotConfig,
    cx: &App,
  ) -> impl Future<Output = Result<()>> + use<> {
    let conn = self.conn.clone();
    let primary = self.primary_wifi(cx).cloned();
    let spare = self
      .spare_wifi(cx)
      .map(|i| (i.path.clone(), i.name.clone()));
    // not ready yet, right after it was added say, but still the hotspot's place
    let ours = self
      .list_interfaces(cx)
      .iter()
      .any(|i| i.name == nl80211::AP_INTERFACE);
    let beside = self.hotspot_beside_wifi(cx);
    let uplink = self.primary_interface(cx).map(|i| i.name.clone());
    let executor = cx.background_executor().clone();
    let (rules, root) = (self.hotspot_rules.clone(), self.root.clone());
    async move {
      // before the password is asked for
      config.validate()?;
      let primary = primary.context("no usable wifi device")?;
      let connected = primary.state == DeviceState::Activated;
      let rules_for = |name: &str| HotspotRules::new(name, uplink.as_deref());
      // set by the first root call, which lets the clients through
      let mut allowed = None;
      let started = async {
        let (device, name) = match spare {
          Some((device, name)) => {
            let rules = rules_for(&name);
            root.run(rules.allow()).await?;
            allowed = Some(rules);
            (device, name)
          }
          None if ours => {
            let rules = rules_for(nl80211::AP_INTERFACE);
            root.run(rules.allow()).await?;
            allowed = Some(rules);
            let ours = actions::wait_for_device(&conn, nl80211::AP_INTERFACE, &executor).await?;
            (ours, nl80211::AP_INTERFACE.to_owned())
          }
          // only a connection is worth an interface beside it, adding it lets the clients through
          None if beside && connected => {
            let rules = rules_for(nl80211::AP_INTERFACE);
            let (uplink, lock) = (rules.uplink.as_deref(), rules.lock_uplink);
            root
              .run(actions::add_ap_interface(&primary.name, uplink, lock))
              .await?;
            allowed = Some(rules);
            let ours = actions::wait_for_device(&conn, nl80211::AP_INTERFACE, &executor).await?;
            (ours, nl80211::AP_INTERFACE.to_owned())
          }
          None => {
            let rules = rules_for(&primary.name);
            root.run(rules.allow()).await?;
            allowed = Some(rules);
            (primary.path.clone(), primary.name.clone())
          }
        };
        // on every path: NM would connect saved networks on it otherwise
        if name == nl80211::AP_INTERFACE {
          actions::no_autoconnect(&conn, device.clone())
            .await
            .log_err()
            .ok();
        }
        let radio = |name: &str| nl80211::radio(name).map(|(phy, _)| phy);
        let shared =
          name != primary.name && radio(&name).is_some_and(|phy| radio(&primary.name) == Some(phy));
        let client = shared.then_some((primary.path.clone(), connected));
        actions::start_hotspot(&conn, device, config, client, &executor).await?;
        anyhow::Ok(())
      }
      .await;
      match (started, allowed) {
        // only now: the listener would take a failed WPA3 try, gone before the WPA2 one, for a
        // hotspot stopped outside corona and remove the rules
        (Ok(()), Some(started)) => {
          *rules.lock().unwrap() = Some(started);
          Ok(())
        }
        (Ok(()), None) => unreachable!("every path lets the clients through first"),
        // nothing stops a hotspot that never ran, its rules would let in the peers of a later
        // client connection on the interface
        (Err(e), allowed) => {
          if allowed.is_some() {
            root.run(actions::disallow_hotspot()).await.log_err().ok();
          }
          Err(e)
        }
      }
    }
  }

  /// the devices connected to the running hotspot, none without one
  pub fn hotspot_clients(
    &self,
    cx: &App,
  ) -> impl Future<Output = Result<Vec<HotspotClient>>> + use<> {
    let interface = self.hotspot(cx).map(|h| h.interface.clone());
    cx.background_executor()
      .spawn(async move { interface.map_or(Ok(Vec::new()), |i| actions::hotspot_clients(&i)) })
  }

  pub fn stop_hotspot(&self, cx: &App) -> impl Future<Output = Result<()>> + use<> {
    let conn = self.conn.clone();
    let profile = self
      .hotspot(cx)
      .map(|h| h.connection_path.clone())
      .context("the hotspot is not running");
    let (rules, root) = (self.hotspot_rules.clone(), self.root.clone());
    async move {
      let profile = profile?;
      // first: a cancelled password prompt keeps the hotspot running with its rules, instead of
      // leaving them on an interface that may connect as a client later
      let ours = rules.lock().unwrap().take();
      if let Err(e) = root.run(actions::disallow_hotspot()).await {
        *rules.lock().unwrap() = ours;
        return Err(e);
      }
      actions::stop_hotspot(&conn, profile).await
    }
  }

  /// keeps corona's firewall rules with the hotspot: removed once it `ended` without corona
  /// stopping it, through nmcli say, moved along to a new `uplink`, renewed when the host's
  /// networks changed (a VPN connecting, say), and taken up again for the Corona Hotspot found
  /// running on `adopt` when corona restarted meanwhile
  fn follow_hotspot_rules(
    &self,
    ended: bool,
    adopt: Option<&str>,
    uplink: Option<&str>,
    networks_changed: bool,
    executor: &BackgroundExecutor,
  ) {
    let mut guard = self.hotspot_rules.lock().unwrap();
    if ended {
      if guard.take().is_some() {
        let root = self.root.clone();
        executor
          .spawn(async move { root.run(actions::disallow_hotspot()).await.log_err().ok() })
          .detach();
      }
      return;
    }
    let rules = match (guard.as_mut(), adopt) {
      (Some(rules), _) => {
        let uplink = uplink.filter(|uplink| *uplink != rules.interface);
        // no route for a moment while switching: the old rules stay until a new uplink is up
        let moved = uplink.is_some() && uplink != rules.uplink.as_deref();
        if moved {
          // recorded only while no hotspot runs: an uplink unseen until now stays locked
          *rules = HotspotRules::new(&rules.interface, uplink);
        } else if !networks_changed {
          return;
        }
        rules.clone()
      }
      (None, Some(interface)) => {
        let rules = HotspotRules::new(interface, uplink);
        *guard = Some(rules.clone());
        rules
      }
      (None, None) => return,
    };
    drop(guard);
    let (current, root) = (self.hotspot_rules.clone(), self.root.clone());
    executor
      .spawn(async move {
        root
          .run(async {
            // a stop or another move came first
            if current.lock().unwrap().as_ref() != Some(&rules) {
              return;
            }
            rules.allow().await.log_err().ok();
          })
          .await
      })
      .detach();
  }

  /// stops the hotspot on `interface` once it had no clients for `minutes`
  fn auto_off(&self, interface: String, minutes: u32, cx: &mut AsyncApp) -> Task<()> {
    let state = self.clone();
    cx.spawn(async move |cx| {
      let limit = Duration::from_secs(60 * u64::from(minutes));
      let mut idle = Duration::ZERO;
      let mut failures = 0;
      while idle < limit {
        cx.background_executor().timer(AUTO_OFF_CHECK).await;
        let interface = interface.clone();
        let stations = cx
          .background_executor()
          .spawn(async move { nl80211::stations(&interface) })
          .await;
        // a transient error keeps the count, a radio without nl80211 stays so
        let Ok(stations) = stations.log_err() else {
          failures += 1;
          if failures == AUTO_OFF_FAILURES {
            return;
          }
          continue;
        };
        failures = 0;
        idle = if stations.is_empty() {
          idle + AUTO_OFF_CHECK
        } else {
          Duration::ZERO
        };
      }
      let stop = cx.update(|cx| state.stop_hotspot(cx));
      stop.await.log_err().ok();
    })
  }

  /// re-activates the Corona Hotspot on corona's interface on the channel of `client`, the wifi
  /// device beside it on one radio, once it stayed on another channel for a moment
  fn follow_channel(&self, client: Interface, cx: &mut AsyncApp) -> Task<()> {
    let state = self.clone();
    cx.spawn(async move |cx| {
      cx.background_executor().timer(ROAM_DEBOUNCE).await;
      let name = client.name.clone();
      let shared = cx
        .background_executor()
        .spawn(async move {
          let radio = |name: &str| nl80211::radio(name).map(|(phy, _)| phy);
          radio(nl80211::AP_INTERFACE).is_some_and(|phy| radio(&name) == Some(phy))
        })
        .await;
      // a radio of its own keeps its channel
      if !shared {
        return;
      }
      let (config, device, conn, executor) = cx.update(|cx| {
        (
          state.hotspot_config(cx),
          state.interface_path(nl80211::AP_INTERFACE, cx),
          state.conn.clone(),
          cx.background_executor().clone(),
        )
      });
      let restarted = async move {
        let config = config.await?.context("the hotspot has no profile")?;
        tracing::info!("the wifi connection moved channel, the hotspot follows");
        let client = Some((client.path, true));
        actions::start_hotspot(&conn, device?, config, client, &executor).await
      };
      // detached: re-activating shows the hotspot activating, which drops this task
      cx.background_executor()
        .spawn(async move { restarted.await.log_err().ok() })
        .detach();
    })
  }

  /// the running hotspot's settings and password, else the saved Corona Hotspot's
  pub fn hotspot_config(
    &self,
    cx: &App,
  ) -> impl Future<Output = Result<Option<HotspotConfig>>> + use<> {
    let (conn, running) = (
      self.conn.clone(),
      self.hotspot(cx).map(|h| h.connection_path.clone()),
    );
    async move { actions::hotspot_config(&conn, running).await }
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

  fn new(conn: &Connection, cx: &mut App) -> Self {
    Self {
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
      hotspot: cx.new(|_| None),
      hotspot_beside_wifi: cx.new(|_| false),
      hotspot_rules: Arc::default(),
      root: RootLock::new(),
      conn: conn.clone(),
    }
  }

  /// Without listening to NetworkManager; tests write the entities themselves
  #[cfg(any(test, feature = "test-support"))]
  pub fn fake(conn: &Connection, cx: &mut App) -> Self {
    Self::new(conn, cx)
  }
}

pub async fn init(cx: &mut App, conn: &Connection) -> Result<()> {
  let state = NetworkManager::new(conn, cx);

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
  use std::{net::Ipv4Addr, time::Duration};

  use corona_utils::test_bus::{TestBus, settle, wait_until};
  use futures_lite::future::{block_on, poll_once};
  use gpui_kit::{self as gpui, TestAppContext};
  use zbus::zvariant::{OwnedValue, Value};

  use super::*;
  use crate::mock::{
    AP0, Active, ETH, MockNm, NM, SETTINGS, SettingsMap, WLAN, World, ap, profile,
  };

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
      assert!(nm.hotspot(cx).is_none());
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

  /// wlan0 runs the saved Corona Hotspot, its connection in `state`
  fn hotspot_world(state: u32) -> World {
    let mut world = World::default();
    let mut hotspot = profile(5, "Corona Hotspot", "802-11-wireless", Some(b"corona"));
    hotspot.mode = Some("ap".into());
    hotspot.band = Some("bg".into());
    hotspot.psk = Some("hunter22".into());
    world.profiles.push(hotspot);
    world.wlan_state = 100;
    world.wlan_active = format!("{NM}/ActiveConnection/3");
    world.active.push(Active {
      path: format!("{NM}/ActiveConnection/3"),
      uuid: "uuid-Corona Hotspot".into(),
      state,
      devices: vec![WLAN.into()],
      connection: format!("{SETTINGS}/5"),
    });
    world
  }

  fn hotspot(cx: &mut TestAppContext) -> Option<HotspotInfo> {
    cx.read(|cx| cx.network_manager().hotspot(cx).cloned())
  }

  #[gpui::test]
  fn detects_a_running_hotspot(cx: &mut TestAppContext) {
    for (state, active) in [(1, false), (2, true)] {
      let (_bus, mock) = start(cx, hotspot_world(state));
      wait_until(cx, |cx| hotspot(cx).is_some());
      assert_eq!(
        hotspot(cx),
        Some(HotspotInfo {
          active,
          ssid: "corona".into(),
          band: HotspotBand::Bg,
          connection_path: OwnedObjectPath::try_from(format!("{SETTINGS}/5")).unwrap(),
          interface: "wlan0".into(),
          // saved before auto-off was settable
          auto_off: Some(10),
          corona: true,
          channel: None,
        })
      );
      // an access point does not scan
      assert!(networks(cx).is_empty());

      let nm = nm(cx);
      let config = block_on(cx.read(|cx| nm.hotspot_config(cx))).unwrap();
      assert_eq!(
        config,
        Some(HotspotConfig {
          ssid: "corona".into(),
          password: Some("hunter22".into()),
          band: HotspotBand::Bg,
          wpa3: false,
          auto_off: Some(10),
          isolate: false,
        })
      );
      block_on(cx.read(|cx| nm.stop_hotspot(cx))).unwrap();
      let calls = mock.calls();
      assert!(calls.contains(&format!("GetSecrets {SETTINGS}/5")));
      assert!(calls.contains(&format!("DeactivateConnection {NM}/ActiveConnection/3")));
      cx.update(|cx| cx.remove_global::<NetworkManager>());
    }
  }

  fn object(path: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(path).unwrap()
  }

  #[gpui::test]
  fn a_hotspot_beside_the_wifi_connection(cx: &mut TestAppContext) {
    // the hotspot runs on corona-ap, wlan0 stays on home's 5 GHz access point
    let mut world = hotspot_world(2);
    world.wlan_active = "/".into();
    world.active.last_mut().unwrap().devices = vec![AP0.into()];
    world.ap0_state = Some(100);
    world.ap0_active = format!("{NM}/ActiveConnection/3");
    world.active_ap = format!("{NM}/AccessPoint/3");
    world.aps[2].frequency = 5180;
    let (_bus, mock) = start(cx, world);
    wait_until(cx, |cx| hotspot(cx).is_some() && !networks(cx).is_empty());
    assert_eq!(networks(cx)[0].3, WifiStatus::Connected);
    // its clients are read from the interface running it
    assert_eq!(hotspot(cx).unwrap().interface, "corona-ap");
    let nm = nm(cx);
    cx.read(|cx| {
      assert_eq!(nm.primary_wifi(cx).unwrap().name, "wlan0");
      assert_eq!(nm.spare_wifi(cx).unwrap().name, "corona-ap");
      // the mock's wlan0 is no radio of this machine
      assert!(!nm.hotspot_beside_wifi(cx));
    });

    let config = HotspotConfig {
      ssid: "corona".into(),
      password: None,
      band: HotspotBand::Bg,
      wpa3: false,
      auto_off: Some(10),
      isolate: false,
    };
    block_on(cx.read(|cx| nm.start_hotspot(config.clone(), cx))).unwrap();
    assert!(
      mock
        .calls()
        .contains(&format!("ActivateConnection {SETTINGS}/5 {AP0} /"))
    );
    // sharing wlan0's radio it follows wlan0's channel, whatever band was asked for
    let executor = cx.executor();
    block_on(actions::start_hotspot(
      &nm.conn,
      object(AP0),
      config,
      Some((object(WLAN), true)),
      &executor,
    ))
    .unwrap();
    let updated = added(&mock);
    assert_eq!(
      text(&updated, "802-11-wireless", "band").as_deref(),
      Some("a")
    );
    assert_eq!(
      get(&updated, "802-11-wireless", "channel"),
      Some(Value::from(36u32).try_to_owned().unwrap())
    );
  }

  fn open_hotspot() -> HotspotConfig {
    HotspotConfig {
      ssid: "corona".into(),
      password: None,
      band: HotspotBand::Bg,
      wpa3: false,
      auto_off: Some(10),
      isolate: false,
    }
  }

  /// advances the fake clock until `task` resolves
  fn finish<T: Unpin + 'static>(cx: &mut TestAppContext, task: &mut gpui::Task<T>) -> T {
    let mut result = None;
    wait_until(cx, |cx| {
      cx.executor().advance_clock(Duration::from_millis(200));
      result = block_on(poll_once(&mut *task));
      result.is_some()
    });
    result.unwrap()
  }

  #[gpui::test]
  fn an_unready_corona_ap_is_waited_for(cx: &mut TestAppContext) {
    // right after wpa_supplicant restarted: corona-ap is not ready yet, wlan0 reconnects
    let mut world = World::default();
    world.wlan_state = 40;
    world.ap0_state = Some(20);
    let (_bus, mock) = start(cx, world);
    wait_until(cx, |cx| {
      cx.read(|cx| cx.network_manager().list_interfaces(cx).len() == 3)
    });
    let nm = nm(cx);
    let mut task = cx
      .executor()
      .spawn(cx.read(|cx| nm.start_hotspot(open_hotspot(), cx)));
    settle(cx);
    mock.world().ap0_state = Some(30);
    finish(cx, &mut task).unwrap();
    let calls = mock.calls();
    assert!(calls.contains(&format!("AddAndActivateConnection {AP0} /")));
    // wlan0 is never taken over
    assert!(!calls.iter().any(|c| c.contains(WLAN)), "{calls:?}");
  }

  #[gpui::test]
  fn waits_for_the_wifi_to_reconnect_before_taking_its_channel(cx: &mut TestAppContext) {
    // dropped by wpa_supplicant restarting, about to come back on home's 5 GHz access point
    let mut world = World::default();
    world.wlan_state = 30;
    world.active_ap = format!("{NM}/AccessPoint/3");
    world.aps[2].frequency = 5180;
    world.ap0_state = Some(30);
    let (_bus, mock) = start(cx, world);
    let (conn, executor) = (nm(cx).conn, cx.executor());
    let mut task = cx.executor().spawn(async move {
      let client = Some((object(WLAN), true));
      actions::start_hotspot(&conn, object(AP0), open_hotspot(), client, &executor).await
    });
    settle(cx);
    assert!(block_on(poll_once(&mut task)).is_none());
    assert!(mock.world().added.is_empty());

    mock.world().wlan_state = 100;
    finish(cx, &mut task).unwrap();
    let added = added(&mock);
    assert_eq!(
      text(&added, "802-11-wireless", "band").as_deref(),
      Some("a")
    );
    assert_eq!(
      get(&added, "802-11-wireless", "channel"),
      Some(Value::from(36u32).try_to_owned().unwrap())
    );
  }

  #[gpui::test]
  fn coronas_interface_is_never_the_primary_wifi(cx: &mut TestAppContext) {
    let mut world = World::default();
    world.wlan_state = 20;
    world.ap0_state = Some(30);
    let (_bus, _mock) = start(cx, world);
    wait_until(cx, |cx| {
      cx.read(|cx| cx.network_manager().list_interfaces(cx).len() == 3)
    });
    cx.read(|cx| {
      let nm = cx.network_manager();
      assert!(nm.primary_wifi(cx).is_none());
      assert_eq!(nm.spare_wifi(cx).unwrap().name, "corona-ap");
    });
  }

  #[gpui::test]
  fn waits_for_networkmanager_to_take_up_an_interface(cx: &mut TestAppContext) {
    let (_bus, mock) = start(cx, World::default());
    // listed, but still unavailable
    mock.world().ap0_state = Some(20);
    let (conn, executor) = (nm(cx).conn, cx.executor());
    let mut task = cx
      .executor()
      .spawn(async move { actions::wait_for_device(&conn, "corona-ap", &executor).await });
    settle(cx);
    assert!(block_on(poll_once(&mut task)).is_none());

    mock.world().ap0_state = Some(30);
    assert_eq!(finish(cx, &mut task).unwrap(), object(AP0));
  }

  #[gpui::test]
  fn starts_a_new_hotspot(cx: &mut TestAppContext) {
    let (_bus, mock) = start(cx, World::default());
    let nm = nm(cx);
    // nothing saved, nothing running
    assert_eq!(block_on(cx.read(|cx| nm.hotspot_config(cx))).unwrap(), None);
    assert_eq!(
      block_on(cx.read(|cx| nm.stop_hotspot(cx)))
        .unwrap_err()
        .to_string(),
      "the hotspot is not running"
    );

    let config = HotspotConfig {
      ssid: "corona".into(),
      password: Some("password".into()),
      band: HotspotBand::A,
      wpa3: false,
      auto_off: None,
      isolate: true,
    };
    block_on(cx.read(|cx| nm.start_hotspot(config.clone(), cx))).unwrap();
    assert!(
      mock
        .calls()
        .contains(&format!("AddAndActivateConnection {WLAN} /"))
    );
    // the listener follows them from now on
    assert!(nm.hotspot_rules.lock().unwrap().is_some());
    let added = added(&mock);
    assert_eq!(
      text(&added, "connection", "id").as_deref(),
      Some("Corona Hotspot")
    );
    assert_eq!(
      get(&added, "connection", "autoconnect"),
      Some(Value::from(false).try_to_owned().unwrap())
    );
    assert_eq!(
      get(&added, "802-11-wireless", "ssid"),
      Some(Value::from(b"corona".to_vec()).try_to_owned().unwrap())
    );
    assert_eq!(
      text(&added, "802-11-wireless", "mode").as_deref(),
      Some("ap")
    );
    assert_eq!(
      text(&added, "802-11-wireless", "band").as_deref(),
      Some("a")
    );
    assert_eq!(text(&added, "ipv4", "method").as_deref(), Some("shared"));
    assert_eq!(
      text(&added, "802-11-wireless-security", "key-mgmt").as_deref(),
      Some("wpa-psk")
    );
    assert_eq!(
      text(&added, "802-11-wireless-security", "psk").as_deref(),
      Some("password")
    );
    // never turns off
    let data = get(&added, "user", "data").unwrap();
    let data = std::collections::HashMap::<String, String>::try_from(data).unwrap();
    assert_eq!(data["corona.auto-off"], "0");
    assert_eq!(text(&added, "ipv6", "method").as_deref(), Some("disabled"));
    assert_eq!(
      text(&added, "802-11-wireless", "cloned-mac-address").as_deref(),
      Some("random")
    );
    assert_eq!(
      get(&added, "802-11-wireless", "ap-isolation"),
      Some(Value::from(1i32).try_to_owned().unwrap())
    );
    assert_eq!(
      get(&added, "802-11-wireless-security", "pmf"),
      Some(Value::from(2i32).try_to_owned().unwrap())
    );
    assert_eq!(actions::auto_off(&added), None);

    // too short a password never reaches NM
    let short = HotspotConfig {
      password: Some("short".into()),
      ..config
    };
    assert!(block_on(cx.read(|cx| nm.start_hotspot(short, cx))).is_err());
    assert_eq!(mock.world().added.len(), 1);
  }

  #[gpui::test]
  fn a_hotspot_failing_after_activation_is_an_error(cx: &mut TestAppContext) {
    // the mock answers every activation with ActiveConnection/1, which NM then drops
    let mut world = World::default();
    world.active[0].state = 4;
    let (_bus, _mock) = start(cx, world);
    let (conn, executor) = (nm(cx).conn, cx.executor());
    let started = block_on(actions::start_hotspot(
      &conn,
      object(WLAN),
      open_hotspot(),
      None,
      &executor,
    ));
    assert!(started.is_err());
    // its rules are gone again, nothing is left for the listener to follow
    let nm = nm(cx);
    assert!(block_on(cx.read(|cx| nm.start_hotspot(open_hotspot(), cx))).is_err());
    assert!(nm.hotspot_rules.lock().unwrap().is_none());
  }

  #[gpui::test]
  fn hotspot_rules_follow_the_uplink(cx: &mut TestAppContext) {
    let (_bus, _mock) = start(cx, World::default());
    let nm = nm(cx);
    let executor = cx.executor();
    let rules = |uplink: Option<&str>| {
      Some(HotspotRules {
        interface: "corona-ap".into(),
        uplink: uplink.map(Into::into),
        // the tests record no forwarding: every uplink is locked
        lock_uplink: uplink.is_some(),
      })
    };
    let follow = |ended, adopt, uplink, changed| {
      nm.follow_hotspot_rules(ended, adopt, uplink, changed, &executor)
    };
    *nm.hotspot_rules.lock().unwrap() = rules(Some("eth0"));
    // no route while switching, or the hotspot's own interface: kept
    follow(false, None, None, false);
    follow(false, None, Some("corona-ap"), true);
    assert_eq!(*nm.hotspot_rules.lock().unwrap(), rules(Some("eth0")));
    follow(false, Some("corona-ap"), Some("wlan0"), false);
    assert_eq!(*nm.hotspot_rules.lock().unwrap(), rules(Some("wlan0")));
    // ended without corona stopping it
    follow(true, None, Some("wlan0"), false);
    assert_eq!(*nm.hotspot_rules.lock().unwrap(), None);
    // none of corona's, nothing to follow
    follow(false, None, Some("eth0"), true);
    assert_eq!(*nm.hotspot_rules.lock().unwrap(), None);
    // the Corona Hotspot an earlier corona left running is taken up
    follow(false, Some("corona-ap"), Some("eth0"), false);
    assert_eq!(*nm.hotspot_rules.lock().unwrap(), rules(Some("eth0")));
  }

  #[gpui::test]
  fn root_calls_take_turns(cx: &mut TestAppContext) {
    let root = RootLock::new();
    let order = Arc::new(Mutex::new(Vec::new()));
    let (first_in, first_wait) = flume::bounded::<()>(1);
    let first = cx.executor().spawn({
      let (root, order) = (root.clone(), order.clone());
      async move {
        root
          .run(async {
            first_wait.recv_async().await.ok();
            order.lock().unwrap().push(1);
          })
          .await
      }
    });
    settle(cx);
    let second = cx.executor().spawn({
      let (root, order) = (root.clone(), order.clone());
      async move { root.run(async { order.lock().unwrap().push(2) }).await }
    });
    settle(cx);
    // waits for the first, which holds the token
    assert!(order.lock().unwrap().is_empty());
    first_in.send(()).unwrap();
    settle(cx);
    drop((first, second));
    assert_eq!(*order.lock().unwrap(), [1, 2]);
  }

  #[gpui::test]
  fn a_failing_wpa3_hotspot_falls_back_to_wpa2(cx: &mut TestAppContext) {
    // the saved hotspot, not running; the mock answers every activation with
    // ActiveConnection/1, which NM then drops
    let mut world = hotspot_world(2);
    world.active.pop();
    world.wlan_active = "/".into();
    world.wlan_state = 30;
    world.active[0].state = 4;
    let (_bus, mock) = start(cx, world);
    let (conn, executor) = (nm(cx).conn, cx.executor());
    let config = HotspotConfig {
      password: Some("password".into()),
      wpa3: true,
      auto_off: Some(30),
      ..open_hotspot()
    };
    let started = block_on(actions::start_hotspot(
      &conn,
      object(WLAN),
      config,
      None,
      &executor,
    ));
    assert!(started.is_err());
    let tried = mock.world().added.clone();
    let key_mgmt = |settings| text(settings, "802-11-wireless-security", "key-mgmt");
    // WPA2 failed too: the profile goes back to the WPA3 asked for
    assert_eq!(
      tried.iter().map(key_mgmt).collect::<Vec<_>>(),
      [
        Some("sae".into()),
        Some("wpa-psk".into()),
        Some("sae".into())
      ]
    );
    assert_eq!(
      get(&tried[0], "802-11-wireless-security", "pmf"),
      Some(Value::from(3i32).try_to_owned().unwrap())
    );
    assert_eq!(
      get(&tried[1], "802-11-wireless-security", "pmf"),
      Some(Value::from(2i32).try_to_owned().unwrap())
    );
    assert_eq!(actions::auto_off(&tried[1]), Some(30));
  }

  #[gpui::test]
  fn reuses_the_saved_hotspot(cx: &mut TestAppContext) {
    let mut world = hotspot_world(2);
    world.active.pop();
    world.wlan_active = "/".into();
    world.wlan_state = 30;
    let (_bus, mock) = start(cx, world);
    settle(cx);
    assert!(hotspot(cx).is_none());
    let nm = nm(cx);
    // not running: the saved one fills the form
    let saved = block_on(cx.read(|cx| nm.hotspot_config(cx)))
      .unwrap()
      .unwrap();
    assert_eq!(saved.password.as_deref(), Some("hunter22"));

    let open = HotspotConfig {
      password: None,
      ..saved
    };
    block_on(cx.read(|cx| nm.start_hotspot(open, cx))).unwrap();
    let calls: Vec<_> = mock
      .calls()
      .into_iter()
      .filter(|c| !c.starts_with("Register") && !c.starts_with("GetSecrets"))
      .collect();
    assert_eq!(
      calls,
      [
        format!("Update {SETTINGS}/5"),
        format!("ActivateConnection {SETTINGS}/5 {WLAN} /"),
      ]
    );
    let updated = added(&mock);
    assert_eq!(
      text(&updated, "connection", "uuid").as_deref(),
      Some("uuid-Corona Hotspot")
    );
    assert!(!updated.contains_key("802-11-wireless-security"));
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
