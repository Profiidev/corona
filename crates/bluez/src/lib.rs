use anyhow::{Context, Result};
use bluez_zbus::{adapter1::Adapter1Proxy, device1::Device1Proxy};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{App, AppContext, Entity, Global};
use zbus::{Connection, proxy::CacheProperties, zvariant::OwnedObjectPath};

use crate::{
  agent::Reply,
  listener::{agent_listener, listener, subscribe},
};

pub use crate::{
  agent::{PairingKind, PairingRequest},
  state::{Adapter, Device},
};

mod agent;
mod listener;
mod snapshot;
mod state;

#[derive(Clone)]
pub struct Bluetooth {
  pub adapter: Entity<Option<Adapter>>,
  pub devices: Entity<Vec<Device>>,
  pub pairing_request: Entity<Option<PairingRequest>>,
  conn: Connection,
}

impl Global for Bluetooth {}

pub trait BluetoothExt {
  fn bluetooth(&self) -> &Bluetooth;
}

impl BluetoothExt for App {
  fn bluetooth(&self) -> &Bluetooth {
    self.global::<Bluetooth>()
  }
}

pub enum PairingAnswer {
  Reject,
  Accept,
  Code(String),
}

impl Bluetooth {
  pub fn adapter<'c>(&self, cx: &'c App) -> Option<&'c Adapter> {
    self.adapter.read(cx).as_ref()
  }

  pub fn list_devices<'c>(&self, cx: &'c App) -> &'c [Device] {
    self.devices.read(cx)
  }

  pub fn device<'c>(&self, address: &str, cx: &'c App) -> Option<&'c Device> {
    self.list_devices(cx).iter().find(|d| d.address == address)
  }

  pub fn pairing_request<'c>(&self, cx: &'c App) -> Option<&'c PairingRequest> {
    self.pairing_request.read(cx).as_ref()
  }

  fn adapter_path(&self, cx: &App) -> Result<OwnedObjectPath> {
    Ok(
      self
        .adapter(cx)
        .context("no Bluetooth adapter")?
        .path
        .clone(),
    )
  }

  fn device_path(&self, address: &str, cx: &App) -> Result<OwnedObjectPath> {
    Ok(
      self
        .device(address, cx)
        .with_context(|| format!("no device {address}"))?
        .path
        .clone(),
    )
  }

  pub fn set_powered(&self, powered: bool, cx: &App) -> impl Future<Output = Result<()>> + use<> {
    let (conn, path) = (self.conn.clone(), self.adapter_path(cx));
    async move { Ok(adapter(&conn, path?).await?.set_powered(powered).await?) }
  }

  pub fn set_discoverable(
    &self,
    discoverable: bool,
    cx: &App,
  ) -> impl Future<Output = Result<()>> + use<> {
    let (conn, path) = (self.conn.clone(), self.adapter_path(cx));
    async move {
      Ok(
        adapter(&conn, path?)
          .await?
          .set_discoverable(discoverable)
          .await?,
      )
    }
  }

  pub fn start_discovery(&self, cx: &App) -> impl Future<Output = Result<()>> + use<> {
    let (conn, path) = (self.conn.clone(), self.adapter_path(cx));
    async move { Ok(adapter(&conn, path?).await?.start_discovery().await?) }
  }

  pub fn stop_discovery(&self, cx: &App) -> impl Future<Output = Result<()>> + use<> {
    let (conn, path) = (self.conn.clone(), self.adapter_path(cx));
    async move { Ok(adapter(&conn, path?).await?.stop_discovery().await?) }
  }

  pub fn connect(&self, address: &str, cx: &App) -> impl Future<Output = Result<()>> + use<> {
    let (conn, path) = (self.conn.clone(), self.device_path(address, cx));
    async move { Ok(device(&conn, path?).await?.connect().await?) }
  }

  pub fn disconnect(&self, address: &str, cx: &App) -> impl Future<Output = Result<()>> + use<> {
    let (conn, path) = (self.conn.clone(), self.device_path(address, cx));
    async move { Ok(device(&conn, path?).await?.disconnect().await?) }
  }

  pub fn pair(&self, address: &str, cx: &App) -> impl Future<Output = Result<()>> + use<> {
    let (conn, path) = (self.conn.clone(), self.device_path(address, cx));
    async move {
      let device = device(&conn, path?).await?;
      device.pair().await?;
      device.set_trusted(true).await?;
      Ok(device.connect().await?)
    }
  }

  pub fn forget(&self, address: &str, cx: &App) -> impl Future<Output = Result<()>> + use<> {
    let conn = self.conn.clone();
    let paths = self.adapter_path(cx).and_then(|adapter| {
      let device = self.device_path(address, cx)?;
      Ok((adapter, device))
    });
    async move {
      let (adapter_path, device_path) = paths?;
      Ok(
        adapter(&conn, adapter_path)
          .await?
          .remove_device(&device_path)
          .await?,
      )
    }
  }

  pub fn answer_pairing(&self, cx: &mut App, answer: PairingAnswer) {
    let request = self.pairing_request.update(cx, |request, cx| {
      cx.notify();
      request.take()
    });
    let Some(request) = request else {
      return;
    };
    let accept = !matches!(answer, PairingAnswer::Reject);
    let code = match answer {
      PairingAnswer::Code(code) => Some(code.trim().to_string()),
      _ => None,
    };
    match request.reply {
      Reply::Accept(reply) => _ = reply.send(accept),
      Reply::PinCode(reply) => _ = reply.send(code.filter(|_| accept)),
      Reply::Passkey(reply) => _ = reply.send(code.and_then(|code| code.parse().ok())),
      Reply::None => {}
    }
  }
}

async fn adapter(conn: &Connection, path: OwnedObjectPath) -> zbus::Result<Adapter1Proxy<'static>> {
  Adapter1Proxy::builder(conn)
    .path(path)?
    .cache_properties(CacheProperties::No)
    .build()
    .await
}

async fn device(conn: &Connection, path: OwnedObjectPath) -> zbus::Result<Device1Proxy<'static>> {
  Device1Proxy::builder(conn)
    .path(path)?
    .cache_properties(CacheProperties::No)
    .build()
    .await
}

pub async fn init(cx: &mut App, conn: &Connection) -> Result<()> {
  let state = Bluetooth {
    adapter: cx.new(|_| None),
    devices: cx.new(|_| Vec::new()),
    pairing_request: cx.new(|_| None),
    conn: conn.clone(),
  };

  // subscribe before the first snapshot so no change can slip in between
  let changes = subscribe(conn).await?;
  listener(cx, conn.clone(), changes, state.clone());
  if let Ok(messages) = agent::register(conn).await.log_err() {
    agent_listener(cx, messages, state.pairing_request.clone());
  }
  cx.set_global(state);

  Ok(())
}
