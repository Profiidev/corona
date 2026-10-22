use anyhow::Result;
use corona_utils::{entity::WriteChangedExt, error::ErrorLogExt, stream::pump};
use cosmic_dbus_networkmanager::interface::{
  NetworkManagerProxy,
  active_connection::ActiveConnectionProxy,
  config::ip4::Ipv4ConfigProxy,
  device::{DeviceProxy, wireless::WirelessDeviceProxy},
  enums::DeviceState,
  settings::{SettingsProxy, connection::ConnectionSettingsProxy},
};
use futures_lite::{Stream, StreamExt, future::poll_once};
use gpui_kit::{App, Entity};
use zbus::{
  Connection, MatchRule, Message, MessageStream, fdo::PropertiesChanged, message::Type,
  names::InterfaceName, proxy::Defaults, zvariant::OwnedObjectPath,
};

use crate::{
  ActiveConnectionState, NetworkManager, SecretRequest, actions,
  agent::AgentEvent,
  forwarding,
  nl80211::{self, AP_INTERFACE},
  snapshot::{WifiFailure, WifiStatus, snapshot},
};

/// interfaces whose PropertiesChanged trigger a snapshot
const WATCHED_INTERFACES: [&Option<InterfaceName<'static>>; 4] = [
  NetworkManagerProxy::INTERFACE,
  DeviceProxy::INTERFACE,
  Ipv4ConfigProxy::INTERFACE,
  ActiveConnectionProxy::INTERFACE,
];

/// interfaces whose own signals trigger a snapshot: access points added / removed,
/// profiles added / removed / updated
const SIGNAL_INTERFACES: [&Option<InterfaceName<'static>>; 3] = [
  WirelessDeviceProxy::INTERFACE,
  SettingsProxy::INTERFACE,
  ConnectionSettingsProxy::INTERFACE,
];

struct StateChange {
  path: OwnedObjectPath,
  state: DeviceState,
  reason: u32,
}

/// NM's signals, and NM starting or quitting
pub async fn subscribe(
  conn: &Connection,
) -> Result<impl Stream<Item = zbus::Result<Message>> + Unpin + use<>> {
  let name = NetworkManagerProxy::DESTINATION.clone().unwrap();
  let rule = MatchRule::builder()
    .msg_type(Type::Signal)
    .sender(name.clone())?
    .path_namespace(NetworkManagerProxy::PATH.clone().unwrap())?
    .build();
  let owner = MatchRule::builder()
    .msg_type(Type::Signal)
    .sender("org.freedesktop.DBus")?
    .interface("org.freedesktop.DBus")?
    .member("NameOwnerChanged")?
    .arg(0, name.as_str())?
    .build();
  let signals = MessageStream::for_match_rule(rule, conn, None).await?;
  let owner = MessageStream::for_match_rule(owner, conn, None).await?;
  Ok(signals.or(owner))
}

pub fn listener(
  cx: &mut App,
  conn: Connection,
  changes: impl Stream<Item = zbus::Result<Message>> + Unpin + 'static,
  state: NetworkManager,
) {
  let mut changes = pump(cx, changes);
  cx.spawn(async move |cx| {
    let mut wifi_path = None;
    let mut connecting_ssid = None;
    let mut failed_path = None;
    // the running hotspot's interface and auto-off minutes, and the task counting its clients
    let mut auto_off = None;
    // dropping it cancels it
    let mut _auto_off_task = None;
    // the channel the hotspot is about to follow the wifi connection to, and the task doing so
    let mut roam = None;
    let mut _roam_task = None;
    // the activated interfaces and VPNs, which route the host's networks
    let mut networks = Vec::new();
    let mut ap_seen = false;
    loop {
      if let Ok(snapshot) = snapshot(&conn).await.log_err() {
        wifi_path = snapshot.primary_wifi.as_ref().map(|i| i.path.clone());
        if failed_path.is_some() && failed_path != wifi_path {
          failed_path = None;
          state.wifi_failure.write_changed(cx, None);
        }
        connecting_ssid = snapshot
          .wifi_networks
          .iter()
          .find(|n| matches!(n.status, WifiStatus::Connecting | WifiStatus::NeedAuth))
          .map(|n| n.ssid.clone());

        let uplink = snapshot.primary_interface.as_ref().map(|i| i.name.clone());
        let routing: Vec<_> = snapshot
          .interfaces
          .iter()
          .filter(|i| i.state == DeviceState::Activated)
          .map(|i| i.name.clone())
          .chain(
            snapshot
              .vpns
              .iter()
              .filter(|v| v.state == ActiveConnectionState::Activated)
              .map(|v| v.uuid.clone()),
          )
          .collect();
        let networks_changed = routing != networks;
        networks = routing;
        if snapshot.hotspot.is_none() {
          // NM may leave forwarding on after a hotspot, so it is recorded before one only
          let names: Vec<_> = snapshot.interfaces.iter().map(|i| i.name.clone()).collect();
          cx.background_executor()
            .spawn(async move { forwarding::observe(names.iter().map(String::as_str)) })
            .detach();
        }
        // NM would connect saved networks on corona's interface, from the moment it appears
        let ap = snapshot
          .interfaces
          .iter()
          .find(|i| i.name == AP_INTERFACE && i.usable())
          .map(|i| i.path.clone());
        if let Some(ap) = ap.clone().filter(|_| !ap_seen) {
          let conn = conn.clone();
          cx.background_executor()
            .spawn(async move { actions::no_autoconnect(&conn, ap).await.log_err().ok() })
            .detach();
        }
        ap_seen = ap.is_some();

        // the primary's channel, when the Corona Hotspot on corona's interface sits on another
        let moved = snapshot
          .hotspot
          .as_ref()
          .filter(|h| h.active && h.corona && h.interface == AP_INTERFACE)
          .and_then(|h| {
            let (_, channel) = actions::channel(snapshot.primary_wifi_frequency?)?;
            (h.channel.is_some_and(|c| c != channel)).then_some(channel)
          })
          .zip(snapshot.primary_wifi.clone());
        if moved.as_ref().map(|(channel, _)| *channel) != roam {
          roam = moved.as_ref().map(|(channel, _)| *channel);
          _roam_task = moved.map(|(_, primary)| state.follow_channel(primary, cx));
        }

        let primary_wifi = snapshot.primary_wifi.as_ref().map(|i| i.name.clone());
        state.interfaces.write_changed(cx, snapshot.interfaces);
        state
          .primary_interface
          .write_changed(cx, snapshot.primary_interface);
        state.connectivity.write_changed(cx, snapshot.connectivity);
        state
          .connectivity_check
          .write_changed(cx, snapshot.connectivity_check);
        state
          .wifi_supported
          .write_changed(cx, snapshot.wifi_supported);
        state.wifi_enabled.write_changed(cx, snapshot.wifi_enabled);
        state.primary_wifi.write_changed(cx, snapshot.primary_wifi);
        state
          .wifi_networks
          .write_changed(cx, snapshot.wifi_networks);
        state.vpns.write_changed(cx, snapshot.vpns);

        let ended = snapshot.hotspot.is_none() && cx.update(|cx| state.hotspot(cx).is_some());
        // a Corona Hotspot left running by an earlier corona
        let adopt = snapshot
          .hotspot
          .as_ref()
          .filter(|h| h.active && h.corona)
          .map(|h| h.interface.as_str());
        state.follow_hotspot_rules(
          ended,
          adopt,
          uplink.as_deref(),
          networks_changed,
          cx.background_executor(),
        );
        // restarts with every hotspot, corona's own or one found running on startup
        let running = snapshot
          .hotspot
          .as_ref()
          .filter(|h| h.active)
          .and_then(|h| Some((h.interface.clone(), h.auto_off?)));
        if running != auto_off {
          _auto_off_task = running
            .clone()
            .map(|(interface, minutes)| state.auto_off(interface, minutes, cx));
          auto_off = running;
        }
        state.hotspot.write_changed(cx, snapshot.hotspot);
        // sysfs and a netlink dump, off the foreground
        let beside = cx
          .background_executor()
          .spawn(async move {
            primary_wifi
              .and_then(|name| nl80211::radio(&name))
              .is_some_and(|(phy, _)| nl80211::station_beside_ap(phy))
          })
          .await;
        state.hotspot_beside_wifi.write_changed(cx, beside);
      }

      let Some(state_changes) = next_changes(&mut changes).await else {
        break;
      };
      for change in state_changes {
        if Some(&change.path) != wifi_path.as_ref() {
          continue;
        }
        match change.state {
          DeviceState::Failed => {
            state.wifi_failure.write_changed(
              cx,
              Some(WifiFailure {
                ssid: connecting_ssid.clone(),
                reason: change.reason.into(),
              }),
            );
            failed_path = Some(change.path);
          }
          DeviceState::Disconnected => {}
          _ => {
            failed_path = None;
            state.wifi_failure.write_changed(cx, None);
          }
        }
      }
    }
  })
  .detach();
}

pub fn agent_listener(
  cx: &mut App,
  events: flume::Receiver<AgentEvent>,
  secret_request: Entity<Option<SecretRequest>>,
  wifi_failure: Entity<Option<WifiFailure>>,
) {
  cx.spawn(async move |cx| {
    while let Ok(event) = events.recv_async().await {
      match event {
        AgentEvent::Request(request) => {
          wifi_failure.write_changed(cx, None);
          secret_request.write(cx, Some(request));
        }
        // only the request NM gives up on closes, a newer one may be showing already
        AgentEvent::Cancel {
          connection,
          setting,
        } => secret_request.update(cx, |request, cx| {
          if request
            .as_ref()
            .is_some_and(|r| r.connection == connection && r.setting == setting)
          {
            *request = None;
            cx.notify();
          }
        }),
      }
    }
  })
  .detach();
}

async fn next_changes(
  changes: &mut (impl Stream<Item = zbus::Result<Message>> + Unpin),
) -> Option<Vec<StateChange>> {
  let mut state_changes = Vec::new();
  while let Some(msg) = changes.next().await {
    let Ok(msg) = msg else { continue };
    state_changes.extend(state_change(&msg));
    if is_relevant(&msg) {
      // drain the rest of the burst already queued, one snapshot covers it
      while let Some(Some(msg)) = poll_once(changes.next()).await {
        state_changes.extend(msg.ok().as_ref().and_then(state_change));
      }
      return Some(state_changes);
    }
  }
  None
}

fn state_change(msg: &Message) -> Option<StateChange> {
  let header = msg.header();
  // StateChanged(new, old, reason) is the only signal of the device interface
  if header.interface() != DeviceProxy::INTERFACE.as_ref() {
    return None;
  }
  let (state, _old, reason): (u32, u32, u32) = msg.body().deserialize().ok()?;
  Some(StateChange {
    path: header.path()?.to_owned().into(),
    state: state.into(),
    reason,
  })
}

fn is_relevant(msg: &Message) -> bool {
  let header = msg.header();
  // NetworkManager (re)started or quit
  if header.member().is_some_and(|m| m == "NameOwnerChanged") {
    return true;
  }
  let interface = header.interface().cloned();
  if SIGNAL_INTERFACES
    .iter()
    .any(|i| i.as_ref() == interface.as_ref())
  {
    return true;
  }
  PropertiesChanged::from_message(msg.clone()).is_some_and(|signal| {
    signal.args().is_ok_and(|args| {
      WATCHED_INTERFACES
        .iter()
        .any(|i| i.as_ref() == Some(&args.interface_name))
    })
  })
}

#[cfg(test)]
mod tests {
  use std::collections::HashMap;

  use zbus::zvariant::Value;

  use super::*;

  fn signal(path: &str, interface: &str, member: &str) -> zbus::message::Builder<'static> {
    Message::signal(path.to_string(), interface.to_string(), member.to_string()).unwrap()
  }

  fn properties_changed(interface: &str) -> Message {
    let changed: HashMap<&str, Value<'_>> = HashMap::from([("State", Value::from(100u32))]);
    signal(
      "/org/freedesktop/NetworkManager",
      "org.freedesktop.DBus.Properties",
      "PropertiesChanged",
    )
    .build(&(interface, changed, Vec::<&str>::new()))
    .unwrap()
  }

  #[test]
  fn device_state_changes() {
    let device = "/org/freedesktop/NetworkManager/Devices/2";
    let msg = signal(
      device,
      "org.freedesktop.NetworkManager.Device",
      "StateChanged",
    )
    .build(&(120u32, 50u32, 7u32))
    .unwrap();
    let change = state_change(&msg).unwrap();
    assert_eq!(
      (change.path.as_str(), change.state, change.reason),
      (device, DeviceState::Failed, 7)
    );
    // a device signal with another body, or another interface, is no state change
    let odd = signal(
      device,
      "org.freedesktop.NetworkManager.Device",
      "StateChanged",
    )
    .build(&("x",))
    .unwrap();
    assert!(state_change(&odd).is_none());
    let wireless = signal(
      device,
      "org.freedesktop.NetworkManager.Device.Wireless",
      "AccessPointAdded",
    )
    .build(&(zbus::zvariant::ObjectPath::from_static_str_unchecked("/ap"),))
    .unwrap();
    assert!(state_change(&wireless).is_none());
    assert!(is_relevant(&wireless));
    assert!(!is_relevant(&msg));
  }

  #[test]
  fn relevant_signals() {
    for interface in [
      "org.freedesktop.NetworkManager",
      "org.freedesktop.NetworkManager.Device",
      "org.freedesktop.NetworkManager.IP4Config",
      "org.freedesktop.NetworkManager.Connection.Active",
    ] {
      assert!(is_relevant(&properties_changed(interface)), "{interface}");
    }
    // access points change strength all the time, that alone is not worth a snapshot
    assert!(!is_relevant(&properties_changed(
      "org.freedesktop.NetworkManager.AccessPoint"
    )));
    for (interface, member) in [
      ("org.freedesktop.NetworkManager.Settings", "NewConnection"),
      (
        "org.freedesktop.NetworkManager.Settings.Connection",
        "Updated",
      ),
    ] {
      let msg = signal(
        "/org/freedesktop/NetworkManager/Settings",
        interface,
        member,
      )
      .build(&())
      .unwrap();
      assert!(is_relevant(&msg), "{interface}");
    }
    let other = signal(
      "/org/freedesktop/NetworkManager",
      "org.example.Other",
      "Thing",
    )
    .build(&())
    .unwrap();
    assert!(!is_relevant(&other));
  }
}
