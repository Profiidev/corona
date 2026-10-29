use anyhow::{Result, bail};
use corona_ipc::{IpcCommand, IpcExt, Switch};
use corona_network_manager::NetworkManagerExt;
use corona_utils::error::ErrorLogExt;
use gpui_kit::App;

pub fn register_commands(cx: &mut App) {
  cx.ipc_mut().register::<Wifi>();
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
