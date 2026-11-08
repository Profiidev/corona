use super::connection::later;
use anyhow::{Result, bail};
use corona_ipc::{IpcCommand, IpcExt, Reply, Switch};
use corona_network_manager::NetworkManagerExt;
use corona_utils::error::spawn_logged;
use gpui_kit::App;

pub(crate) fn register(cx: &mut App) {
  cx.ipc_mut().register::<Wifi>();
}

pub struct Wifi;

/// on or off afterwards, and whether that is a change
fn target(switch: Switch, cx: &App) -> Result<(bool, bool)> {
  let network = cx.network_manager();
  if !network.wifi_supported(cx) {
    bail!("no Wi-Fi device");
  }
  let current = network.wifi_enabled(cx);
  Ok(
    switch
      .target(current)
      .map_or((current, false), |enabled| (enabled, true)),
  )
}

impl IpcCommand for Wifi {
  const COMMAND: &'static str = "wifi:switch";

  type Payload = Switch;
  /// on or off afterwards
  type Response = bool;

  fn handle(switch: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let (enabled, change) = target(switch, cx)?;
    if change {
      let set = cx.network_manager().set_wifi_enabled(enabled);
      spawn_logged(cx, set);
    }
    Ok(enabled)
  }

  // the CLI waits for NetworkManager, a refused switch is an error there
  fn reply(switch: Self::Payload, cx: &mut App) -> Reply<Self::Response> {
    let (enabled, change) = match target(switch, cx) {
      Ok(target) => target,
      Err(e) => return later(async { Err(e) }),
    };
    let set = change.then(|| cx.network_manager().set_wifi_enabled(enabled));
    later(async move {
      if let Some(set) = set {
        set.await?;
      }
      Ok(enabled)
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use corona_utils::{Buses, test_bus::TestBus};
  use gpui_kit::{self as gpui, TestAppContext};

  #[gpui::test]
  async fn wifi_switch_without_device(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (_bus, conn) = TestBus::with_conn();
    let buses = Buses {
      system: conn.clone(),
      session: conn,
    };
    cx.update(|cx| {
      register(cx);
      let ex = cx.foreground_executor().clone();
      ex.block_on(corona_network_manager::init(cx, &buses))
        .unwrap();

      assert!(Wifi::handle(Switch::Status, cx).is_err());
    });
    let reply = cx.update(|cx| Wifi::reply(Switch::On, cx)).await;
    assert!(reply.is_err());
  }
}
