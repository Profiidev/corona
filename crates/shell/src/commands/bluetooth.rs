use anyhow::{Context, Result, anyhow, bail};
use clap::ValueEnum;
use corona_bluez::{BluetoothExt, Device};
use corona_ipc::{IpcCommand, IpcServer, Reply};
use gpui_kit::App;
use serde::{Deserialize, Serialize};

pub fn register_commands(server: &mut IpcServer) {
  server
    .register::<ListDevices>()
    .register::<DeviceAction>()
    .register::<Discover>();
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeviceInfo {
  pub address: String,
  pub name: String,
  pub paired: bool,
  pub connected: bool,
  pub battery: Option<u8>,
}

pub struct ListDevices;

impl IpcCommand for ListDevices {
  const COMMAND: &'static str = "bluetooth:list";

  type Payload = ();
  type Response = Vec<DeviceInfo>;

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    Ok(
      cx.bluetooth()
        .list_devices(cx)
        .iter()
        .map(|d| DeviceInfo {
          address: d.address.clone(),
          name: d.name.clone(),
          paired: d.paired,
          connected: d.connected,
          battery: d.battery,
        })
        .collect(),
    )
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
pub enum Action {
  Connect,
  Disconnect,
  Pair,
  Forget,
}

/// The device with this address or name
pub fn find(which: &str, cx: &App) -> Result<Device> {
  cx.bluetooth()
    .list_devices(cx)
    .iter()
    .find(|d| d.address.eq_ignore_ascii_case(which) || d.name.eq_ignore_ascii_case(which))
    .cloned()
    .with_context(|| format!("no Bluetooth device {which}"))
}

/// Connects, disconnects, pairs or forgets a device by address or name
pub struct DeviceAction;

impl IpcCommand for DeviceAction {
  const COMMAND: &'static str = "bluetooth:device";

  type Payload = (Action, String);
  type Response = ();

  fn handle(_: Self::Payload, _: &mut App) -> Result<Self::Response> {
    bail!("bluetooth:device answers asynchronously")
  }

  fn reply((action, which): Self::Payload, cx: &mut App) -> Reply<Self::Response> {
    let device = match find(&which, cx) {
      Ok(device) => device,
      Err(e) => return Box::pin(async move { Err(e) }),
    };
    let bluetooth = cx.bluetooth();
    let address = device.address.as_str();
    match action {
      Action::Connect => Box::pin(bluetooth.connect(address, cx)),
      Action::Disconnect => Box::pin(bluetooth.disconnect(address, cx)),
      Action::Pair => Box::pin(bluetooth.pair(address, cx)),
      Action::Forget => Box::pin(bluetooth.forget(address, cx)),
    }
  }
}

/// Starts or stops looking for devices to pair
pub struct Discover;

impl IpcCommand for Discover {
  const COMMAND: &'static str = "bluetooth:discover";

  type Payload = bool;
  type Response = ();

  fn handle(_: Self::Payload, _: &mut App) -> Result<Self::Response> {
    bail!("bluetooth:discover answers asynchronously")
  }

  fn reply(on: Self::Payload, cx: &mut App) -> Reply<Self::Response> {
    let bluetooth = cx.bluetooth();
    if bluetooth.adapter(cx).is_none() {
      return Box::pin(async { Err(anyhow!("no Bluetooth adapter")) });
    }
    match on {
      true => Box::pin(bluetooth.start_discovery(cx)),
      false => Box::pin(bluetooth.stop_discovery(cx)),
    }
  }
}
