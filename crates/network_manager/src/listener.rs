use anyhow::Result;
use corona_utils::{entity::WriteChangedExt, error::ErrorLogExt};
use cosmic_dbus_networkmanager::interface::{
  NetworkManagerProxy,
  active_connection::ActiveConnectionProxy,
  config::ip4::Ipv4ConfigProxy,
  device::{DeviceProxy, wireless::WirelessDeviceProxy},
  enums::DeviceState,
  settings::{SettingsProxy, connection::ConnectionSettingsProxy},
};
use futures_lite::{StreamExt, future::poll_once};
use gpui_kit::{App, Entity};
use zbus::{
  Connection, MatchRule, Message, MessageStream, fdo::PropertiesChanged, message::Type,
  names::InterfaceName, proxy::Defaults, zvariant::OwnedObjectPath,
};

use crate::{
  NetworkManager, SecretRequest,
  agent::AgentEvent,
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

pub async fn subscribe(conn: &Connection) -> Result<MessageStream> {
  let rule = MatchRule::builder()
    .msg_type(Type::Signal)
    .sender(NetworkManagerProxy::DESTINATION.clone().unwrap())?
    .path_namespace(NetworkManagerProxy::PATH.clone().unwrap())?
    .build();
  Ok(MessageStream::for_match_rule(rule, conn, None).await?)
}

pub fn listener(cx: &mut App, conn: Connection, mut changes: MessageStream, state: NetworkManager) {
  cx.spawn(async move |cx| {
    let mut wifi_path = None;
    let mut connecting_ssid = None;
    let mut failed_path = None;
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
      let request = match event {
        AgentEvent::Request(request) => {
          wifi_failure.write_changed(cx, None);
          Some(request)
        }
        AgentEvent::Cancel => None,
      };
      secret_request.write(cx, request);
    }
  })
  .detach();
}

async fn next_changes(changes: &mut MessageStream) -> Option<Vec<StateChange>> {
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
  let interface = msg.header().interface().cloned();
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
