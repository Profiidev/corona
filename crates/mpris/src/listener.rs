use anyhow::Result;
use corona_utils::error::ErrorLogExt;
use futures_lite::{Stream, StreamExt, future::poll_once};
use gpui_kit::{App, AsyncApp, Entity};
use mpris2_zbus::enumerator::Enumerator;
use zbus::{Connection, MatchRule, MessageStream, message::Type};

use crate::{Mpris, snapshot::snapshot, state::pick_active};

const PLAYER_PATH: &str = "/org/mpris/MediaPlayer2";

/// fires on players appearing or quitting, on their property changes and on seeks
pub async fn subscribe(conn: &Connection) -> Result<impl Stream<Item = ()> + Unpin + use<>> {
  let rule = MatchRule::builder()
    .msg_type(Type::Signal)
    .path(PLAYER_PATH)?
    .build();
  let signals = MessageStream::for_match_rule(rule, conn, None).await?;
  let players = Enumerator::new(conn).await?.receive_changes().await?;
  Ok(signals.map(|_| ()).or(players.map(|_| ())))
}

pub fn listener(
  cx: &mut App,
  conn: Connection,
  mut changes: impl Stream<Item = ()> + Unpin + 'static,
  state: Mpris,
) {
  cx.spawn(async move |cx| {
    loop {
      if let Ok(players) = snapshot(&conn).await.log_err() {
        let active = cx.update(|cx| state.active.read(cx).clone());
        write_changed(cx, &state.active, pick_active(&players, active.as_deref()));
        write_changed(cx, &state.players, players);
      }

      if changes.next().await.is_none() {
        break;
      }
      // drain the rest of the burst already queued, one snapshot covers it
      while let Some(Some(())) = poll_once(changes.next()).await {}
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
