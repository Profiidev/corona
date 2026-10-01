use corona_mpris as mpris;
use corona_mpris::{Mpris, MprisExt};
use gpui_kit::App;
use gpui_shell::HostModule;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use ts_rs::TS;

use crate::{
  host_fn::{Cx, Glob, Module},
  module::{Subscribe, Subscriptions, read, watch},
};
use corona_macros::named;

#[derive(Serialize, TS)]
#[serde(rename_all = "snake_case")]
enum PlaybackStatus {
  Playing,
  Paused,
  Stopped,
}

#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
enum LoopStatus {
  /// Playback stops after the last track.
  None,
  /// The current track repeats.
  Track,
  /// The whole playlist repeats.
  Playlist,
}

#[derive(Serialize, TS)]
struct Player {
  /// The bus name, the id the actions take.
  name: String,
  /// A human readable name, like "Spotify".
  identity: String,
  status: PlaybackStatus,
  title: Option<String>,
  artists: Vec<String>,
  album: Option<String>,
  /// Usually a `file://` or `https://` URL.
  art_url: Option<String>,
  /// In seconds.
  length: Option<f64>,
  /// In seconds, moved on to the time of the read while playing.
  position: f64,
  /// From 0 to 1, null when the player has no volume.
  volume: Option<f64>,
  shuffle: Option<bool>,
  loop_status: Option<LoopStatus>,
  can_control: bool,
  can_play: bool,
  can_pause: bool,
  can_go_next: bool,
  can_go_previous: bool,
  can_seek: bool,
}

impl From<mpris::LoopStatus> for LoopStatus {
  fn from(status: mpris::LoopStatus) -> Self {
    match status {
      mpris::LoopStatus::None => LoopStatus::None,
      mpris::LoopStatus::Track => LoopStatus::Track,
      mpris::LoopStatus::Playlist => LoopStatus::Playlist,
    }
  }
}

impl From<LoopStatus> for mpris::LoopStatus {
  fn from(status: LoopStatus) -> Self {
    match status {
      LoopStatus::None => mpris::LoopStatus::None,
      LoopStatus::Track => mpris::LoopStatus::Track,
      LoopStatus::Playlist => mpris::LoopStatus::Playlist,
    }
  }
}

impl From<&mpris::Player> for Player {
  fn from(player: &mpris::Player) -> Self {
    Self {
      name: player.name.clone(),
      identity: player.identity.clone(),
      status: match player.status {
        mpris::PlaybackStatus::Playing => PlaybackStatus::Playing,
        mpris::PlaybackStatus::Paused => PlaybackStatus::Paused,
        mpris::PlaybackStatus::Stopped => PlaybackStatus::Stopped,
      },
      title: player.title.clone(),
      artists: player.artists.clone(),
      album: player.album.clone(),
      art_url: player.art_url.clone(),
      length: player.length.map(|length| length.as_secs_f64()),
      position: player.position().as_secs_f64(),
      volume: player.volume,
      shuffle: player.shuffle,
      loop_status: player.loop_status.map(LoopStatus::from),
      can_control: player.can_control,
      can_play: player.can_play,
      can_pause: player.can_pause,
      can_go_next: player.can_go_next,
      can_go_previous: player.can_go_previous,
      can_seek: player.can_seek,
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Updates {
  Players,
  Active,
}

impl From<Updates> for super::Updates {
  fn from(value: Updates) -> Self {
    super::Updates::Mpris(value)
  }
}

pub fn module(reads: &Subscriptions, subs: &mut Vec<Subscribe>, cx: &mut App) -> HostModule {
  let state = cx.mpris();
  // `activePlayer` reads the player too, so its track and status changes re-render it as well
  subs.push(watch(reads, Updates::Active.into(), state.players.clone()));

  Module::new("corona/mpris")
    .func(read(
      reads,
      subs,
      "listPlayers",
      Updates::Players,
      state.players.clone(),
      |cx| {
        let players = cx.mpris().list_players(cx);
        players.iter().map(Player::from).collect::<Vec<_>>()
      },
    ))
    .func(read(
      reads,
      subs,
      "activePlayer",
      Updates::Active,
      state.active.clone(),
      |cx| cx.mpris().active_player(cx).map(Player::from),
    ))
    .func(named!(
      "setActivePlayer",
      /// Shows `name` as the active player until another one starts playing.
      |cx: &mut App, name: String| cx.mpris().clone().set_active(&name, cx)
    ))
    .func(named!("playPause", |mpris: Glob<Mpris>, name: String| {
      mpris.play_pause(&name)
    }))
    .func(named!("next", |mpris: Glob<Mpris>, name: String| mpris.next(&name)))
    .func(named!("previous", |mpris: Glob<Mpris>, name: String| mpris
      .previous(&name)))
    .func(named!(
      "setPosition",
      /// Jumps to `seconds` into the current track.
      |cx: Cx, mpris: Glob<Mpris>, name: String, seconds: f64| {
        mpris.set_position(&name, Duration::from_secs_f64(seconds.max(0.0)), &cx)
      }
    ))
    .func(named!(
      "setVolume",
      /// `volume` from 0 to 1.
      |mpris: Glob<Mpris>, name: String, volume: f64| mpris.set_volume(&name, volume)
    ))
    .func(named!(
      "setShuffle",
      |mpris: Glob<Mpris>, name: String, shuffle: bool| mpris.set_shuffle(&name, shuffle)
    ))
    .func(named!(
      "setLoopStatus",
      |mpris: Glob<Mpris>, name: String, status: LoopStatus| {
        mpris.set_loop_status(&name, status.into())
      }
    ))
    .into()
}
