use anyhow::Result;
use bluez_zbus::agent1::Message;
use corona_utils::error::ErrorLogExt;
use futures_channel::mpsc;
use futures_lite::{StreamExt, future::poll_once};
use gpui_kit::{App, AsyncApp, Entity};
use zbus::{Connection, MatchRule, MessageStream, message::Type};

use crate::{
  Bluetooth, PairingRequest,
  agent::{AgentEvent, event},
  snapshot::snapshot,
};

pub async fn subscribe(conn: &Connection) -> Result<MessageStream> {
  let rule = MatchRule::builder()
    .msg_type(Type::Signal)
    .sender("org.bluez")?
    .build();
  Ok(MessageStream::for_match_rule(rule, conn, None).await?)
}

pub fn listener(cx: &mut App, conn: Connection, mut changes: MessageStream, state: Bluetooth) {
  cx.spawn(async move |cx| {
    loop {
      let snapshot = snapshot(&conn).await.log_err().ok();
      let (adapter, devices) = snapshot.map_or((None, Vec::new()), |s| (s.adapter, s.devices));
      write_changed(cx, &state.adapter, adapter);
      write_changed(cx, &state.devices, devices);

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
  mut messages: mpsc::Receiver<Message>,
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

fn write_changed<T: PartialEq + 'static>(cx: &mut AsyncApp, entity: &Entity<T>, next: T) {
  cx.update(|cx| {
    if *entity.read(cx) != next {
      entity.write(cx, next);
    }
  });
}
