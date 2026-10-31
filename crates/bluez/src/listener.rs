use anyhow::Result;
use bluez_zbus::agent1::Message;
use corona_utils::{
  dbus::{owner_changed, sender_signals, subscribe as subscribe_to},
  entity::WriteChangedExt,
  error::ErrorLogExt,
  stream::{next_burst, pump},
};
use futures_channel::mpsc;
use futures_lite::{Stream, StreamExt};
use gpui_kit::{App, Entity};
use zbus::Connection;

use crate::{
  Bluetooth, PairingRequest,
  agent::{AgentEvent, event},
  snapshot::snapshot,
};

const BLUEZ: &str = "org.bluez";

/// fires on bluez's signals and on bluetoothd starting or quitting
pub async fn subscribe(conn: &Connection) -> Result<impl Stream<Item = ()> + Unpin + use<>> {
  Ok(subscribe_to(conn, [sender_signals(BLUEZ)?, owner_changed(BLUEZ)?]).await?)
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
      let snapshot = snapshot(&conn).await.log_err().ok();
      let (adapter, devices) = snapshot.map_or((None, Vec::new()), |s| (s.adapter, s.devices));
      state.adapter.write_changed(cx, adapter);
      state.devices.write_changed(cx, devices);

      if next_burst(&mut changes).await.is_none() {
        break;
      }
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
