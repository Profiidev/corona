use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use corona_bluez::BluetoothExt;
use corona_ipc::{IpcCommand, IpcServer};
use corona_network_manager::NetworkManagerExt;
use corona_utils::error::ErrorLogExt;
use gpui_kit::App;
use serde::{Deserialize, Serialize};

pub fn register_commands(server: &mut IpcServer) {
  server.register::<Wifi>().register::<Bluetooth>();
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, ValueEnum)]
pub enum Switch {
  /// Turn it on
  On,
  /// Turn it off
  Off,
  /// Toggle it
  Toggle,
  /// Print on or off
  Status,
}

impl Switch {
  /// The state to switch to from `current`, none for [`Switch::Status`]
  fn target(self, current: bool) -> Option<bool> {
    match self {
      Switch::On => Some(true),
      Switch::Off => Some(false),
      Switch::Toggle => Some(!current),
      Switch::Status => None,
    }
  }
}

fn spawn(task: impl Future<Output = Result<()>> + 'static, cx: &mut App) {
  cx.spawn(async move |_| {
    let _ = task.await.log_err();
  })
  .detach();
}

pub struct Wifi;

impl IpcCommand for Wifi {
  const COMMAND: &'static str = "wifi:switch";

  type Payload = Switch;
  /// on or off afterwards
  type Response = bool;

  fn handle(switch: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let network = cx.network_manager().clone();
    if !network.wifi_supported(cx) {
      bail!("no Wi-Fi device");
    }
    let current = network.wifi_enabled(cx);
    let Some(enabled) = switch.target(current) else {
      return Ok(current);
    };
    spawn(network.set_wifi_enabled(enabled), cx);
    Ok(enabled)
  }
}

pub struct Bluetooth;

impl IpcCommand for Bluetooth {
  const COMMAND: &'static str = "bluetooth:switch";

  type Payload = Switch;
  /// on or off afterwards
  type Response = bool;

  fn handle(switch: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let bluetooth = cx.bluetooth().clone();
    let current = bluetooth
      .adapter(cx)
      .context("no Bluetooth adapter")?
      .powered;
    let Some(powered) = switch.target(current) else {
      return Ok(current);
    };
    spawn(bluetooth.set_powered(powered, cx), cx);
    Ok(powered)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn switch_target() {
    for current in [false, true] {
      assert_eq!(Switch::On.target(current), Some(true));
      assert_eq!(Switch::Off.target(current), Some(false));
      assert_eq!(Switch::Toggle.target(current), Some(!current));
      assert_eq!(Switch::Status.target(current), None);
    }
  }
}
