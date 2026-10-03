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
/// Playing beats paused beats stopped, the current player stays on a tie.
/// Keeps a player's old position stamp when the fresh read is where the old one predicts, so a
/// snapshot of a player that only moved on with playback compares equal and notifies nobody.
pub(crate) fn keep_positions(fresh: &mut [Player], old: &[Player]) {
  const SLACK: Duration = Duration::from_millis(500);
  for player in fresh {
    let Some(prev) = old.iter().find(|p| p.name == player.name) else {
      continue;
    };
    let predicted = prev.position_after(player.position_at.duration_since(prev.position_at));
    if prev.status == player.status
      && prev.rate == player.rate
      && predicted.abs_diff(player.position) <= SLACK
    {
      player.position = prev.position;
      player.position_at = prev.position_at;
    }
  }
}

pub(crate) fn pick_active(players: &[Player], current: Option<&str>) -> Option<String> {
  let rank = |p: &Player| match p.status {
    PlaybackStatus::Playing => 2,
    PlaybackStatus::Paused => 1,
    PlaybackStatus::Stopped => 0,
  };
  let best = players.iter().map(rank).max()?;
  players
    .iter()
    .find(|p| Some(p.name.as_str()) == current && rank(p) == best)
    .or_else(|| players.iter().find(|p| rank(p) == best))
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
  fn positions_kept_while_on_track() {
    let old = player("a", PlaybackStatus::Playing);
    let mut fresh = old.clone();
    fresh.position_at = old.position_at + Duration::from_secs(2);
    fresh.position = old.position + Duration::from_millis(2100);
    keep_positions(std::slice::from_mut(&mut fresh), std::slice::from_ref(&old));
    assert_eq!(fresh, old);

    // a seek is a real change
    fresh.position = Duration::from_secs(60);
    keep_positions(std::slice::from_mut(&mut fresh), std::slice::from_ref(&old));
    assert_eq!(fresh.position, Duration::from_secs(60));
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
    // a paused player with a track beats a stopped one, even the current
    let players = [player("a", Stopped), player("b", Paused)];
    assert_eq!(pick_active(&players, None).as_deref(), Some("b"));
    assert_eq!(pick_active(&players, Some("a")).as_deref(), Some("b"));
  }
}
