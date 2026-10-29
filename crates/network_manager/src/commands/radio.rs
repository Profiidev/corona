use crate::NetworkManagerExt;
use anyhow::{Result, bail};
use corona_ipc::{IpcCommand, IpcExt, Switch};
use corona_utils::error::spawn_logged;
use gpui_kit::App;

pub(crate) fn register(cx: &mut App) {
  cx.ipc_mut().register::<Wifi>();
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
    spawn_logged(cx, network.set_wifi_enabled(enabled));
    Ok(enabled)
  }
}
