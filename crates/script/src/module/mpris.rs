use std::time::Duration;

use corona_js::{
  Cx, Glob, Json, ModuleBuilder, PluginRef, ScriptModule, Throws, host_module, track,
};
use corona_mpris::{self as mpris, MprisExt};
use gpui_kit::App;
use serde::{Deserialize, Serialize};
use specta::{Type, specta};

#[derive(Serialize, Type)]
#[serde(rename_all = "snake_case")]
enum PlaybackStatus {
  Playing,
  Paused,
  Stopped,
}

#[derive(Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
enum LoopStatus {
  /// Playback stops after the last track.
  None,
  /// The current track repeats.
  Track,
  /// The whole playlist repeats.
  Playlist,
}

#[derive(Serialize, Type)]
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

/// A negative position is the track start, one past `Duration::MAX` saturates.
fn seek_target(seconds: f64) -> Duration {
  Duration::try_from_secs_f64(seconds.max(0.0)).unwrap_or(Duration::MAX)
}

type Mprises = Glob<mpris::Mpris>;

/// Shows `name` as the active player until another one starts playing.
#[specta]
fn set_active_player(mut cx: Cx, name: String) {
  cx.mpris().clone().set_active(&name, &mut cx)
}

#[specta]
async fn play_pause(mpris: Mprises, name: String) -> Throws<()> {
  mpris.play_pause(&name).await.into()
}

#[specta]
async fn play(mpris: Mprises, name: String) -> Throws<()> {
  mpris.play(&name).await.into()
}

#[specta]
async fn pause(mpris: Mprises, name: String) -> Throws<()> {
  mpris.pause(&name).await.into()
}

#[specta]
async fn next(mpris: Mprises, name: String) -> Throws<()> {
  mpris.next(&name).await.into()
}

#[specta]
async fn previous(mpris: Mprises, name: String) -> Throws<()> {
  mpris.previous(&name).await.into()
}

/// Jumps to `seconds` into the current track.
#[specta]
async fn set_position(cx: Cx, mpris: Mprises, name: String, seconds: f64) -> Throws<()> {
  let set = mpris.set_position(&name, seek_target(seconds), &cx);
  set.await.into()
}

/// `volume` from 0 to 1.
#[specta]
async fn set_volume(mpris: Mprises, name: String, volume: f64) -> Throws<()> {
  mpris.set_volume(&name, volume).await.into()
}

#[specta]
async fn set_shuffle(mpris: Mprises, name: String, shuffle: bool) -> Throws<()> {
  mpris.set_shuffle(&name, shuffle).await.into()
}

#[specta]
async fn set_loop_status(mpris: Mprises, name: String, status: Json<LoopStatus>) -> Throws<()> {
  mpris.set_loop_status(&name, status.0.into()).await.into()
}

/// `corona/mpris`
pub struct Mpris;

impl ScriptModule for Mpris {
  const NAME: &str = "mpris";
  const DOC: &str = "Media players.";

  fn module(_: &PluginRef, m: &mut ModuleBuilder, _: &mut App) {
    m.watch(
      "listPlayers",
      "",
      |cx| cx.mpris().players.clone(),
      |cx| {
        let players = cx.mpris().list_players(cx);
        Json(players.iter().map(Player::from).collect::<Vec<_>>())
      },
    );
    m.watch(
      "activePlayer",
      "The player shown as active; null without one.",
      // its track and status change with the players
      |cx| cx.mpris().players.clone(),
      |cx| {
        track(&cx.mpris().active);
        Json(cx.mpris().active_player(cx).map(Player::from))
      },
    );
    host_module!(m, {
      sync: [set_active_player],
      async: [
        play_pause,
        play,
        pause,
        next,
        previous,
        set_position,
        set_volume,
        set_shuffle,
        set_loop_status,
      ],
    });
  }
}

#[cfg(test)]
mod tests {
  use std::time::Instant;

  use super::*;

  fn player(status: mpris::PlaybackStatus) -> mpris::Player {
    mpris::Player {
      name: "org.mpris.MediaPlayer2.spotify".into(),
      identity: "Spotify".into(),
      desktop_entry: None,
      status,
      title: Some("Song".into()),
      artists: vec!["A".into(), "B".into()],
      album: None,
      art_url: None,
      length: Some(Duration::from_millis(180_500)),
      track_id: None,
      position: Duration::from_secs(42),
      position_at: Instant::now(),
      rate: 1.0,
      volume: Some(0.5),
      shuffle: None,
      loop_status: Some(mpris::LoopStatus::Track),
      can_control: true,
      can_play: true,
      can_pause: false,
      can_go_next: true,
      can_go_previous: false,
      can_seek: true,
    }
  }

  #[test]
  fn loop_status_both_ways() {
    let all = [
      (mpris::LoopStatus::None, "none"),
      (mpris::LoopStatus::Track, "track"),
      (mpris::LoopStatus::Playlist, "playlist"),
    ];
    for (status, name) in all {
      assert_eq!(
        serde_json::to_value(LoopStatus::from(status)).unwrap(),
        name
      );
      let parsed: LoopStatus = serde_json::from_value(name.into()).unwrap();
      assert_eq!(mpris::LoopStatus::from(parsed), status);
    }
  }

  #[test]
  fn players() {
    let all = [
      (mpris::PlaybackStatus::Paused, "paused"),
      (mpris::PlaybackStatus::Stopped, "stopped"),
      (mpris::PlaybackStatus::Playing, "playing"),
    ];
    for (status, name) in all {
      let json = serde_json::to_value(Player::from(&player(status))).unwrap();
      assert_eq!(json["status"], name);
      assert_eq!(json["identity"], "Spotify");
      assert_eq!(json["length"], 180.5);
      assert_eq!(json["loop_status"], "track");
      assert_eq!(json["artists"][1], "B");
      assert!(json["shuffle"].is_null());
      assert_eq!(json["can_pause"], false);
      // moves on only while playing
      let position = json["position"].as_f64().unwrap();
      assert!((42.0..=180.5).contains(&position), "{position}");
    }

    let mut paused = player(mpris::PlaybackStatus::Paused);
    paused.length = None;
    paused.loop_status = None;
    let json = serde_json::to_value(Player::from(&paused)).unwrap();
    assert!(json["length"].is_null());
    assert!(json["loop_status"].is_null());
    assert_eq!(json["position"], 42.0);
  }

  #[test]
  fn seek_targets() {
    assert_eq!(seek_target(12.5), Duration::from_millis(12_500));
    assert_eq!(seek_target(0.0), Duration::ZERO);
    assert_eq!(seek_target(-5.0), Duration::ZERO);
    assert_eq!(seek_target(f64::NAN), Duration::ZERO);
  }

  #[test]
  fn seek_to_huge_position_saturates() {
    assert!(std::panic::catch_unwind(|| seek_target(1e300)).is_ok());
  }
}
