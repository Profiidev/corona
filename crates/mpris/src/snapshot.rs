use std::{
  collections::HashMap,
  time::{Duration, Instant},
};

use anyhow::Result;
use corona_utils::error::ErrorLogExt;
use mpris2_zbus::{
  bindings::{media_player::MediaPlayer2Proxy, player::PlayerProxy},
  enumerator::Enumerator,
  media_player::MediaPlayer,
  metadata::Metadata,
  player::Player as MprisPlayer,
};
use zbus::{Connection, names::OwnedBusName, proxy::CacheProperties};

use crate::state::Player;

pub(crate) async fn player_proxy(conn: &Connection, name: OwnedBusName) -> Result<MprisPlayer> {
  Ok(
    PlayerProxy::builder(conn)
      .destination(name)?
      .cache_properties(CacheProperties::No)
      .build()
      .await?
      .into(),
  )
}

pub async fn snapshot(conn: &Connection) -> Result<Vec<Player>> {
  let mut players = Vec::new();
  for name in Enumerator::new(conn).await?.players().await? {
    // a player that quits while being read is left for the next snapshot
    if let Ok(player) = read_player(conn, name).await.log_err() {
      players.push(player);
    }
  }
  // the bus lists names in no particular order
  players.sort_by(|a, b| a.identity.cmp(&b.identity).then(a.name.cmp(&b.name)));
  Ok(players)
}

async fn read_player(conn: &Connection, name: OwnedBusName) -> Result<Player> {
  let root: MediaPlayer = MediaPlayer2Proxy::builder(conn)
    .destination(name.clone())?
    .cache_properties(CacheProperties::No)
    .build()
    .await?
    .into();
  let player = player_proxy(conn, name.clone()).await?;

  let metadata = player
    .metadata()
    .await
    .unwrap_or_else(|_| Metadata::from(HashMap::<String, String>::new()));
  // optional properties: players without them answer with an error
  let position = player.position().await.ok().flatten();
  Ok(Player {
    identity: root.identity().await.unwrap_or_else(|_| name.to_string()),
    desktop_entry: root.desktop_entry().await.ok(),
    status: player.playback_status().await?,
    title: metadata.title().filter(|s| !s.is_empty()),
    artists: metadata.artists().unwrap_or_default(),
    album: metadata.album().filter(|s| !s.is_empty()),
    art_url: metadata.art_url().filter(|s| !s.is_empty()),
    length: metadata
      .length()
      .map(|length| Duration::from_micros(length.as_micros().max(0) as u64)),
    track_id: metadata.track_id(),
    position: position.map_or(Duration::ZERO, |p| {
      Duration::from_micros(p.as_micros().max(0) as u64)
    }),
    position_at: Instant::now(),
    rate: player.rate().await.ok().flatten().unwrap_or(1.0),
    volume: player.volume().await.ok(),
    shuffle: player.shuffle().await.ok().flatten(),
    loop_status: player.loop_status().await.ok().flatten(),
    can_control: player.can_control().await.unwrap_or(false),
    can_play: player.can_play().await.unwrap_or(false),
    can_pause: player.can_pause().await.unwrap_or(false),
    can_go_next: player.can_go_next().await.unwrap_or(false),
    can_go_previous: player.can_go_previous().await.unwrap_or(false),
    can_seek: player.can_seek().await.unwrap_or(false),
    name: name.to_string(),
  })
}

#[cfg(test)]
mod tests {
  /// reads the players on this session bus: `cargo test -p corona_mpris -- --ignored --nocapture`
  #[test]
  #[ignore]
  fn live_snapshot() {
    zbus::block_on(async {
      let conn = zbus::Connection::session().await.unwrap();
      for player in super::snapshot(&conn).await.unwrap() {
        println!("{player:#?}");
      }
    });
  }
}
