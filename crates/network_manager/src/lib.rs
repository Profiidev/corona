pub use agent::{Secret, SecretKind, SecretRequest};

use anyhow::{Context, Result};
use gpui_kit::{App, AppContext, Entity, Global};
use zbus::{Connection, zvariant::OwnedObjectPath};

use crate::listener::{agent_listener, listener, subscribe};

pub use crate::{
  actions::{EnterpriseConfig, HiddenSecurity},
  snapshot::{WifiFailure, WifiNetwork},
  state::{Interface, Vpn},
};
pub use cosmic_dbus_networkmanager::interface::enums::{
  ActiveConnectionState, DeviceState, NmConnectivityState,
};

mod actions;
mod agent;
mod listener;
mod snapshot;
mod state;

#[derive(Clone)]
pub struct NetworkManager {
  pub interfaces: Entity<Vec<Interface>>,
  pub primary_interface: Entity<Option<Interface>>,
  pub connectivity: Entity<NmConnectivityState>,
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

  pub async fn set_wifi_enabled(&self, enabled: bool) -> Result<()> {
    actions::set_wifi_enabled(&self.conn, enabled).await
  }

  pub async fn rescan(&self, cx: &App) -> Result<()> {
    actions::rescan(&self.conn, self.wifi_path(cx)?).await
  }

  pub async fn connect_wifi(&self, ssid: String, cx: &App) -> Result<()> {
    actions::connect_wifi(&self.conn, self.wifi_path(cx)?, ssid).await
  }

  pub async fn forget_wifi(&self, ssid: String, cx: &App) -> Result<()> {
    actions::forget_wifi(&self.conn, self.wifi_path(cx)?, ssid).await
  }

  pub async fn join_hidden_wifi(
    &self,
    ssid: String,
    security: HiddenSecurity,
    password: Option<String>,
    cx: &App,
  ) -> Result<()> {
    actions::join_hidden_wifi(&self.conn, self.wifi_path(cx)?, ssid, security, password).await
  }

  pub async fn connect_vpn(&self, uuid: String) -> Result<()> {
    actions::connect_vpn(&self.conn, uuid).await
  }

  pub async fn disconnect_vpn(&self, uuid: String) -> Result<()> {
    actions::disconnect_vpn(&self.conn, uuid).await
  }

  /// creates an 802.1X profile and connects the primary wifi device with it
  pub async fn connect_enterprise_wifi(&self, config: EnterpriseConfig, cx: &App) -> Result<()> {
    actions::connect_enterprise_wifi(&self.conn, self.wifi_path(cx)?, config).await
  }

  /// activates the best saved profile of an interface
  pub async fn connect(&self, interface: &str, cx: &App) -> Result<()> {
    actions::connect_device(&self.conn, self.interface_path(interface, cx)?).await
  }

  /// disconnects an interface, NM does not autoconnect it again until asked to
  pub async fn disconnect(&self, interface: &str, cx: &App) -> Result<()> {
    actions::disconnect_device(&self.conn, self.interface_path(interface, cx)?).await
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
  agent_listener(cx, events, state.secret_request.clone());
  cx.set_global(state);

  Ok(())
}
