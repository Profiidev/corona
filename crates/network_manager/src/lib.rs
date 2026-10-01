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

  pub fn connect_wifi(&self, ssid: String, cx: &App) -> impl Future<Output = Result<()>> + use<> {
    let (conn, device) = (self.conn.clone(), self.wifi_path(cx));
    async move { actions::connect_wifi(&conn, device?, ssid).await }
  }

  pub fn forget_wifi(&self, ssid: String, cx: &App) -> impl Future<Output = Result<()>> + use<> {
    let (conn, device) = (self.conn.clone(), self.wifi_path(cx));
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
  let events = agent::register(conn).await?;
  agent_listener(
    cx,
    events,
    state.secret_request.clone(),
    state.wifi_failure.clone(),
  );
  cx.set_global(state);

  Ok(())
}
