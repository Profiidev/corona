use std::{
  collections::HashMap,
  time::{Duration, Instant},
};

use anyhow::{Result, bail};
use corona_utils::error::ErrorLogExt;
use futures::{future::join_all, join};
use gpui_kit::AsyncApp;
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

/// the fastest playback a rate is believed to have
const MAX_RATE: f64 = 100.0;

/// a player that does not answer in time is left out, so it cannot hold up the others
const READ_TIMEOUT: Duration = Duration::from_secs(2);

pub async fn snapshot(conn: &Connection, cx: &AsyncApp) -> Result<Vec<Player>> {
  let names = Enumerator::new(conn).await?.players().await?;
  let reads = names.into_iter().map(|name| {
    futures_lite::future::or(read_player(conn, name), async {
      cx.background_executor().timer(READ_TIMEOUT).await;
      bail!("a player did not answer in time")
    })
  });
  // a player that quits while being read is left for the next snapshot
  let mut players: Vec<_> = join_all(reads)
    .await
    .into_iter()
    .filter_map(|player| player.log_err().ok())
    .collect();
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

  let (metadata, position, identity, status, rate) = join!(
    player.metadata(),
    player.position(),
    root.identity(),
    player.playback_status(),
    player.rate(),
  );
  let (volume, shuffle, loop_status) =
    join!(player.volume(), player.shuffle(), player.loop_status());
  let (can_control, can_play, can_pause, can_go_next, can_go_previous, can_seek) = join!(
    player.can_control(),
    player.can_play(),
    player.can_pause(),
    player.can_go_next(),
    player.can_go_previous(),
    player.can_seek(),
  );
  let metadata = metadata.unwrap_or_else(|_| Metadata::from(HashMap::<String, String>::new()));
  // optional properties: players without them answer with an error
  let position = position.ok().flatten();
  Ok(Player {
    identity: identity.unwrap_or_else(|_| name.to_string()),
    status: status?,
    title: metadata.title().filter(|s| !s.is_empty()),
    artists: metadata.artists().unwrap_or_default(),
    album: metadata.album().filter(|s| !s.is_empty()),
    art_url: metadata.art_url().filter(|s| !s.is_empty()),
    length: metadata
      .length()
      .map(|length| Duration::from_micros(length.as_micros().max(0) as u64))
      .filter(|l| !l.is_zero()),
    track_id: metadata.track_id(),
    position: position.map_or(Duration::ZERO, |p| {
      Duration::from_micros(p.as_micros().max(0) as u64)
    }),
    position_at: Instant::now(),
    // players report whatever they like, a rate that is not sane plays at normal speed
    rate: rate
      .ok()
      .flatten()
      .filter(|r| (0.0..=MAX_RATE).contains(r))
      .unwrap_or(1.0),
    volume: volume
      .ok()
      .filter(|v| v.is_finite())
      .map(|v| v.clamp(0.0, 1.0)),
    shuffle: shuffle.ok().flatten(),
    loop_status: loop_status.ok().flatten(),
    can_control: can_control.unwrap_or(false),
    can_play: can_play.unwrap_or(false),
    can_pause: can_pause.unwrap_or(false),
    can_go_next: can_go_next.unwrap_or(false),
    can_go_previous: can_go_previous.unwrap_or(false),
    can_seek: can_seek.unwrap_or(false),
    name: name.to_string(),
  })
}
