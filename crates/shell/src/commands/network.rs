use anyhow::{Context, Result, anyhow, bail};
use clap::ValueEnum;
use corona_ipc::{IpcCommand, IpcServer, Reply};
use corona_network_manager::{
  ActiveConnectionState, HiddenSecurity, HotspotConfig, NetworkManagerExt, ScanResult, WifiStatus,
};
use gpui_kit::App;
use serde::{Deserialize, Serialize};

use crate::commands::radio::Switch;

pub fn register_commands(server: &mut IpcServer) {
  server
    .register::<ListWifi>()
    .register::<ConnectWifi>()
    .register::<DisconnectWifi>()
    .register::<ForgetWifi>()
    .register::<JoinHidden>()
    .register::<Rescan>()
    .register::<ListVpns>()
    .register::<SwitchVpn>()
    .register::<Hotspot>()
    .register::<Portal>();
}

fn later<T: 'static>(future: impl Future<Output = Result<T>> + 'static) -> Reply<T> {
  Box::pin(future)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WifiInfo {
  pub ssid: String,
  pub strength: u8,
  pub secured: bool,
  /// connected, connecting, saved or new
  pub status: String,
}

pub struct ListWifi;

impl IpcCommand for ListWifi {
  const COMMAND: &'static str = "wifi:list";

  type Payload = ();
  type Response = Vec<WifiInfo>;

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    Ok(
      cx.network_manager()
        .list_wifi_networks(cx)
        .iter()
        .map(|n| WifiInfo {
          ssid: n.ssid.clone(),
          strength: n.strength,
          secured: n.secured,
          status: match n.status {
            WifiStatus::Connected => "connected",
            WifiStatus::Connecting | WifiStatus::NeedAuth => "connecting",
            WifiStatus::Saved => "saved",
            WifiStatus::New => "new",
          }
          .to_string(),
        })
        .collect(),
    )
  }
}

/// Connects to a network; a new secured one asks for its password
pub struct ConnectWifi;

impl IpcCommand for ConnectWifi {
  const COMMAND: &'static str = "wifi:connect";

  type Payload = String;
  type Response = ();

  fn handle(_: Self::Payload, _: &mut App) -> Result<Self::Response> {
    bail!("wifi:connect answers asynchronously")
  }

  fn reply(ssid: Self::Payload, cx: &mut App) -> Reply<Self::Response> {
    later(cx.network_manager().connect_wifi(ssid, cx))
  }
}

pub struct DisconnectWifi;

impl IpcCommand for DisconnectWifi {
  const COMMAND: &'static str = "wifi:disconnect";

  type Payload = ();
  type Response = ();

  fn handle(_: Self::Payload, _: &mut App) -> Result<Self::Response> {
    bail!("wifi:disconnect answers asynchronously")
  }

  fn reply(_: Self::Payload, cx: &mut App) -> Reply<Self::Response> {
    let nm = cx.network_manager();
    match nm.primary_wifi(cx).map(|i| i.name.clone()) {
      Some(name) => later(nm.disconnect(&name, cx)),
      None => later(async { Err(anyhow!("no Wi-Fi device")) }),
    }
  }
}

pub struct ForgetWifi;

impl IpcCommand for ForgetWifi {
  const COMMAND: &'static str = "wifi:forget";

  type Payload = String;
  type Response = ();

  fn handle(_: Self::Payload, _: &mut App) -> Result<Self::Response> {
    bail!("wifi:forget answers asynchronously")
  }

  fn reply(ssid: Self::Payload, cx: &mut App) -> Reply<Self::Response> {
    later(cx.network_manager().forget_wifi(ssid, cx))
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
pub enum Security {
  Open,
  Wpa,
  Wpa3,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HiddenRequest {
  pub ssid: String,
  pub security: Security,
  pub password: Option<String>,
}

pub struct JoinHidden;

impl IpcCommand for JoinHidden {
  const COMMAND: &'static str = "wifi:join_hidden";

  type Payload = HiddenRequest;
  type Response = ();

  fn handle(_: Self::Payload, _: &mut App) -> Result<Self::Response> {
    bail!("wifi:join_hidden answers asynchronously")
  }

  fn reply(request: Self::Payload, cx: &mut App) -> Reply<Self::Response> {
    let security = match request.security {
      Security::Open => HiddenSecurity::Open,
      Security::Wpa => HiddenSecurity::Wpa,
      Security::Wpa3 => HiddenSecurity::Wpa3,
    };
    if security != HiddenSecurity::Open && request.password.as_deref().is_none_or(str::is_empty) {
      return later(async { Err(anyhow!("a secured network needs a password")) });
    }
    later(
      cx.network_manager()
        .join_hidden_wifi(request.ssid, security, request.password, cx),
    )
  }
}

/// Scans for networks; true when the scan finished in time
pub struct Rescan;

impl IpcCommand for Rescan {
  const COMMAND: &'static str = "wifi:rescan";

  type Payload = ();
  type Response = bool;

  fn handle(_: Self::Payload, _: &mut App) -> Result<Self::Response> {
    bail!("wifi:rescan answers asynchronously")
  }

  fn reply(_: Self::Payload, cx: &mut App) -> Reply<Self::Response> {
    let scan = cx.network_manager().rescan(cx);
    later(async move { Ok(scan.await? == ScanResult::Done) })
  }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VpnInfo {
  pub uuid: String,
  pub name: String,
  pub active: bool,
}

pub struct ListVpns;

impl IpcCommand for ListVpns {
  const COMMAND: &'static str = "vpn:list";

  type Payload = ();
  type Response = Vec<VpnInfo>;

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    Ok(
      cx.network_manager()
        .list_vpns(cx)
        .iter()
        .map(|v| VpnInfo {
          uuid: v.uuid.clone(),
          name: v.name.clone(),
          active: matches!(
            v.state,
            ActiveConnectionState::Activated | ActiveConnectionState::Activating
          ),
        })
        .collect(),
    )
  }
}

/// A VPN by name or uuid: on, off, toggle or status
pub struct SwitchVpn;

impl IpcCommand for SwitchVpn {
  const COMMAND: &'static str = "vpn:switch";

  type Payload = (String, Switch);
  /// On afterwards
  type Response = bool;

  fn handle(_: Self::Payload, _: &mut App) -> Result<Self::Response> {
    bail!("vpn:switch answers asynchronously")
  }

  fn reply((which, switch): Self::Payload, cx: &mut App) -> Reply<Self::Response> {
    let nm = cx.network_manager();
    let Some(vpn) = nm
      .list_vpns(cx)
      .iter()
      .find(|v| v.uuid == which || v.name.eq_ignore_ascii_case(&which))
      .cloned()
    else {
      return later(async move { Err(anyhow!("no VPN {which}")) });
    };
    let current = matches!(
      vpn.state,
      ActiveConnectionState::Activated | ActiveConnectionState::Activating
    );
    let Some(on) = switch.target(current) else {
      return later(async move { Ok(current) });
    };
    let task = match on {
      true => futures::future::Either::Left(nm.connect_vpn(vpn.uuid)),
      false => futures::future::Either::Right(nm.disconnect_vpn(vpn.uuid)),
    };
    later(async move {
      task.await?;
      Ok(on)
    })
  }
}

/// The hotspot with its saved settings, or new ones
pub struct Hotspot;

impl IpcCommand for Hotspot {
  const COMMAND: &'static str = "hotspot:switch";

  type Payload = Switch;
  /// On afterwards
  type Response = bool;

  fn handle(_: Self::Payload, _: &mut App) -> Result<Self::Response> {
    bail!("hotspot:switch answers asynchronously")
  }

  fn reply(switch: Self::Payload, cx: &mut App) -> Reply<Self::Response> {
    let nm = cx.network_manager().clone();
    let current = nm.hotspot(cx).is_some();
    let Some(on) = switch.target(current) else {
      return later(async move { Ok(current) });
    };
    if !on {
      let stop = nm.stop_hotspot(cx);
      return later(async move {
        stop.await?;
        Ok(false)
      });
    }
    if !nm.wifi_enabled(cx) {
      return later(async { Err(anyhow!("Wi-Fi is off")) });
    }
    let load = nm.hotspot_config(cx);
    let start = cx.spawn(async move |cx| {
      let config = match load.await? {
        Some(config) => config,
        None => HotspotConfig::generated().context("no hotspot settings")?,
      };
      cx.update(|cx| cx.network_manager().start_hotspot(config, cx))
        .await?;
      Ok(true)
    });
    later(start)
  }
}

/// Opens the captive portal's login page
pub struct Portal;

impl IpcCommand for Portal {
  const COMMAND: &'static str = "network:portal";

  type Payload = ();
  type Response = ();

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    cx.network_manager().open_portal(cx);
    Ok(())
  }
}
