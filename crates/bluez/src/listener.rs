use anyhow::Result;
use bluez_zbus::agent1::Message;
use corona_utils::{entity::WriteChangedExt, error::ErrorLogExt};
use futures_channel::mpsc;
use futures_lite::{Stream, StreamExt, future::poll_once};
use gpui_kit::{App, Entity};
use zbus::{Connection, MatchRule, MessageStream, message::Type};

use crate::{
  Bluetooth, PairingRequest,
  agent::{AgentEvent, event},
  snapshot::snapshot,
};

const BLUEZ: &str = "org.bluez";

/// fires on bluez's signals and on bluetoothd starting or quitting
pub async fn subscribe(conn: &Connection) -> Result<impl Stream<Item = ()> + Unpin + use<>> {
  let signals = MatchRule::builder()
    .msg_type(Type::Signal)
    .sender(BLUEZ)?
    .build();
  let owner = MatchRule::builder()
    .msg_type(Type::Signal)
    .sender("org.freedesktop.DBus")?
    .interface("org.freedesktop.DBus")?
    .member("NameOwnerChanged")?
    .arg(0, BLUEZ)?
    .build();
  let signals = MessageStream::for_match_rule(signals, conn, None).await?;
  let owner = MessageStream::for_match_rule(owner, conn, None).await?;
  Ok(signals.map(|_| ()).or(owner.map(|_| ())))
}

pub fn listener(
  cx: &mut App,
  conn: Connection,
  mut changes: impl Stream<Item = ()> + Unpin + 'static,
  state: Bluetooth,
) {
  cx.spawn(async move |cx| {
    loop {
      let snapshot = snapshot(&conn).await.log_err().ok();
      let (adapter, devices) = snapshot.map_or((None, Vec::new()), |s| (s.adapter, s.devices));
      state.adapter.write_changed(cx, adapter);
      state.devices.write_changed(cx, devices);

      if changes.next().await.is_none() {
        break;
      }
      // drain the rest of the burst already queued, one snapshot covers it
      while let Some(Some(_)) = poll_once(changes.next()).await {}
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
