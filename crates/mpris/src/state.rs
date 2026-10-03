use std::{
  path::PathBuf,
  time::{Duration, Instant},
};

use gpui_kit::ImageSource;

pub use mpris2_zbus::player::{LoopStatus, PlaybackStatus};
use zbus::zvariant::OwnedObjectPath;

#[derive(Clone, Debug, PartialEq)]
pub struct Player {
  /// the bus name, like `org.mpris.MediaPlayer2.spotify`
  pub name: String,
  /// a human readable name, like "Spotify"
  pub identity: String,
  pub desktop_entry: Option<String>,
  pub status: PlaybackStatus,
  pub title: Option<String>,
  pub artists: Vec<String>,
  pub album: Option<String>,
  pub art_url: Option<String>,
  pub length: Option<Duration>,
  pub track_id: Option<OwnedObjectPath>,
  /// the position at `position_at`, see `Player::position` for the current one
  pub position: Duration,
  pub position_at: Instant,
  pub rate: f64,
  /// None when the player has no volume
  pub volume: Option<f64>,
  pub shuffle: Option<bool>,
  pub loop_status: Option<LoopStatus>,
  pub can_control: bool,
  pub can_play: bool,
  pub can_pause: bool,
  pub can_go_next: bool,
  pub can_go_previous: bool,
  pub can_seek: bool,
}

impl Player {
  pub fn art_source(&self) -> Option<ImageSource> {
    let url = self.art_url.as_deref()?;
    if let Some(path) = url.strip_prefix("file://") {
      Some(PathBuf::from(path).into())
    } else if url.starts_with("https://") || url.starts_with("http://") {
      Some(url.into())
    } else {
      None
    }
  }

  /// players only report the position when it jumps, in between it moves with the rate
  pub fn position(&self) -> Duration {
    self.position_after(self.position_at.elapsed())
  }

  fn position_after(&self, elapsed: Duration) -> Duration {
    if self.status != PlaybackStatus::Playing {
      return self.position;
    }
    let position = self.position + elapsed.mul_f64(self.rate.max(0.0));
    self.length.map_or(position, |length| position.min(length))
  }
}

/// keeps the current player unless another one started playing, so pausing does not jump away
pub(crate) fn pick_active(players: &[Player], current: Option<&str>) -> Option<String> {
  let current = current.and_then(|name| players.iter().find(|p| p.name == name));
  let playing = players.iter().find(|p| p.status == PlaybackStatus::Playing);
  match (current, playing) {
    (Some(current), _) if current.status == PlaybackStatus::Playing => Some(current),
    (_, Some(playing)) => Some(playing),
    (Some(current), None) => Some(current),
    (None, None) => players.first(),
  }
  .map(|p| p.name.clone())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn player(name: &str, status: PlaybackStatus) -> Player {
    Player {
      name: name.into(),
      identity: name.into(),
      desktop_entry: None,
      status,
      title: None,
      artists: Vec::new(),
      album: None,
      art_url: None,
      length: Some(Duration::from_secs(100)),
      track_id: None,
      position: Duration::from_secs(10),
      position_at: Instant::now(),
      rate: 1.0,
      volume: None,
      shuffle: None,
      loop_status: None,
      can_control: true,
      can_play: true,
      can_pause: true,
      can_go_next: true,
      can_go_previous: true,
      can_seek: true,
    }
  }

  #[test]
  fn position() {
    let playing = player("a", PlaybackStatus::Playing);
    assert_eq!(
      playing.position_after(Duration::from_secs(5)),
      Duration::from_secs(15)
    );
    // clamped to the track length
    assert_eq!(
      playing.position_after(Duration::from_secs(500)),
      Duration::from_secs(100)
    );
    let paused = player("a", PlaybackStatus::Paused);
    assert_eq!(
      paused.position_after(Duration::from_secs(5)),
      Duration::from_secs(10)
    );
  }

  #[test]
  fn active() {
    use PlaybackStatus::*;
    let players = [player("a", Paused), player("b", Playing)];
    assert_eq!(pick_active(&players, None).as_deref(), Some("b"));
    // another player playing takes over from a paused one
    assert_eq!(pick_active(&players, Some("a")).as_deref(), Some("b"));
    let players = [player("a", Paused), player("b", Paused)];
    assert_eq!(pick_active(&players, Some("b")).as_deref(), Some("b"));
    assert_eq!(pick_active(&players, Some("gone")).as_deref(), Some("a"));
    assert_eq!(pick_active(&[], Some("a")), None);
  }
}
