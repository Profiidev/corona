use std::time::Duration;

use anyhow::Result;
use bluez_zbus::agent1::Message;
use corona_utils::{
  dbus::{owner_changed, sender_signals, subscribe as subscribe_to},
  entity::WriteChangedExt,
  error::ErrorLogExt,
  stream::{next_burst, pump},
};
use futures_channel::mpsc;
use futures_lite::{Stream, StreamExt, future};
use gpui_kit::{App, Entity};
use zbus::{Connection, fdo::DBusProxy};

use crate::{
  Bluetooth, PairingRequest,
  agent::{AgentEvent, event, register},
  snapshot::snapshot,
};

const BLUEZ: &str = "org.bluez";
const RETRIES: usize = 5;
const RETRY: Duration = Duration::from_secs(1);
const DEBOUNCE: Duration = Duration::from_millis(250);

/// fires on bluez's signals and on bluetoothd starting or quitting
pub async fn subscribe(conn: &Connection) -> Result<impl Stream<Item = ()> + Unpin + use<>> {
  Ok(subscribe_to(conn, [sender_signals(BLUEZ)?, owner_changed(BLUEZ)?]).await?)
}

/// fires on bluetoothd starting or quitting
pub async fn owners(conn: &Connection) -> Result<impl Stream<Item = ()> + Unpin + use<>> {
  Ok(subscribe_to(conn, [owner_changed(BLUEZ)?]).await?)
}

async fn has_owner(conn: &Connection) -> Result<bool> {
  Ok(
    DBusProxy::new(conn)
      .await?
      .name_has_owner(BLUEZ.try_into()?)
      .await?,
  )
}

/// Registers the agent now and with every bluetoothd that starts after, retrying a start that
/// bluetoothd is not yet ready for
pub fn keep_registered(
  cx: &mut App,
  conn: Connection,
  owners: impl Stream<Item = ()> + Unpin + 'static,
  request: Entity<Option<PairingRequest>>,
) {
  let mut owners = pump(cx, owners);
  cx.spawn(async move |cx| {
    loop {
      // a bluetoothd that quit has nothing to register with
      let alive = has_owner(&conn).await.unwrap_or(false);
      // a pairing of the bluetoothd that quit can no longer be answered, even when a new one
      // is already back
      request.write(cx, None);
      if alive {
        for _ in 0..RETRIES {
          if register(&conn).await.log_err().is_ok() {
            break;
          }
          cx.background_executor().timer(RETRY).await;
        }
      }
      if next_burst(&mut owners).await.is_none() {
        break;
      }
    }
  })
  .detach();
}

pub fn listener(
  cx: &mut App,
  conn: Connection,
  changes: impl Stream<Item = ()> + Unpin + 'static,
  state: Bluetooth,
) {
  let mut changes = pump(cx, changes);
  cx.spawn(async move |cx| {
    loop {
      // a call to the name would start a bluetoothd that was stopped on purpose
      let snapshot = if has_owner(&conn).await.unwrap_or(false) {
        snapshot(&conn).await.log_err().ok()
      } else {
        None
      };
      let (adapter, devices) = snapshot.map_or((None, Vec::new()), |s| (s.adapter, s.devices));
      state.adapter.write_changed(cx, adapter);
      state.devices.write_changed(cx, devices);

      if next_burst(&mut changes).await.is_none() {
        break;
      }
      // discovery floods RSSI changes: let them pile up and rebuild once
      cx.background_executor().timer(DEBOUNCE).await;
      while future::poll_once(changes.next()).await.flatten().is_some() {}
    }
  })
  .detach();
}

pub fn agent_listener(
  cx: &mut App,
  mut messages: mpsc::UnboundedReceiver<Message>,
  request: Entity<Option<PairingRequest>>,
) {
  cx.spawn(async move |cx| {
    while let Some(message) = messages.next().await {
      let Some(event) = event(message) else {
        continue;
      };
      let next = match event {
        AgentEvent::Request(request) => Some(request),
        AgentEvent::Cancel => None,
      };
      request.write(cx, next);
    }
  })
  .detach();
}
