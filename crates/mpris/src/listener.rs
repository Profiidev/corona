use anyhow::Result;
use corona_utils::{
  entity::WriteChangedExt,
  error::ErrorLogExt,
  stream::{next_burst, pump},
};
use futures_lite::{Stream, StreamExt};
use gpui_kit::App;
use mpris2_zbus::enumerator::Enumerator;
use zbus::{Connection, MatchRule, MessageStream, message::Type};

use crate::{
  Mpris,
  snapshot::snapshot,
  state::{keep_positions, pick_active},
};

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
  changes: impl Stream<Item = ()> + Unpin + 'static,
  state: Mpris,
) {
  let mut changes = pump(cx, changes);
  cx.spawn(async move |cx| {
    loop {
      if let Ok(mut players) = snapshot(&conn, cx).await.log_err() {
        let active = cx.update(|cx| {
          let old = state.players.read(cx);
          keep_positions(&mut players, old);
          pick_active(&players, state.active.read(cx).as_deref(), old)
        });
        // players first, so an observer of the active one finds it
        state.players.write_changed(cx, players);
        state.active.write_changed(cx, active);
      }

      if next_burst(&mut changes).await.is_none() {
        break;
      }
    }
  })
  .detach();
}
