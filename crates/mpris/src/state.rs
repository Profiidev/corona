use std::{
  ffi::OsString,
  os::unix::ffi::OsStringExt,
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
      let path = path.strip_prefix("localhost").unwrap_or(path);
      Some(PathBuf::from(OsString::from_vec(percent_decode(path))).into())
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

/// `%20` and friends as their bytes; a stray `%` stays as it is
fn percent_decode(text: &str) -> Vec<u8> {
  let bytes = text.as_bytes();
  let mut out = Vec::with_capacity(bytes.len());
  let mut i = 0;
  while i < bytes.len() {
    let hex = bytes
      .get(i + 1..i + 3)
      .and_then(|h| std::str::from_utf8(h).ok())
      .and_then(|h| u8::from_str_radix(h, 16).ok());
    match (bytes[i], hex) {
      (b'%', Some(byte)) => {
        out.push(byte);
        i += 3;
      }
      (byte, _) => {
        out.push(byte);
        i += 1;
      }
    }
  }
  out
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

  fn with_art(url: Option<&str>) -> Player {
    Player {
      art_url: url.map(Into::into),
      ..player("a", PlaybackStatus::Playing)
    }
  }

  /// where the art loads from: a path or a URI
  fn art(player: Player) -> Option<String> {
    match player.art_source()? {
      ImageSource::Resource(gpui_kit::Resource::Path(path)) => {
        Some(format!("path {}", path.display()))
      }
      ImageSource::Resource(gpui_kit::Resource::Uri(uri)) => Some(format!("uri {uri}")),
      _ => Some("other".into()),
    }
  }

  #[test]
  fn art_sources() {
    assert_eq!(art(with_art(None)), None);
    assert_eq!(
      art(with_art(Some("file:///tmp/cover.png"))).as_deref(),
      Some("path /tmp/cover.png")
    );
    assert_eq!(
      art(with_art(Some("https://i.scdn.co/image/x"))).as_deref(),
      Some("uri https://i.scdn.co/image/x")
    );
    assert_eq!(
      art(with_art(Some("http://localhost/x.jpg"))).as_deref(),
      Some("uri http://localhost/x.jpg")
    );
    for unsupported in [
      "data:image/png;base64,AAAA",
      "/tmp/cover.png",
      "ftp://x",
      "",
    ] {
      assert_eq!(art(with_art(Some(unsupported))), None, "{unsupported}");
    }
  }

  #[test]
  fn file_urls_are_decoded() {
    assert_eq!(
      art(with_art(Some("file:///home/u/My%20Music/%C3%84rzte.jpg"))).as_deref(),
      Some("path /home/u/My Music/Ärzte.jpg")
    );
    assert_eq!(
      art(with_art(Some("file://localhost/a.png"))).as_deref(),
      Some("path /a.png")
    );
    // broken escapes stay as they are
    assert_eq!(
      art(with_art(Some("file:///100%/%zz%2"))).as_deref(),
      Some("path /100%/%zz%2")
    );
    assert_eq!(percent_decode("%ff"), [0xff]);
  }

  #[test]
  fn position_edges() {
    let at = |status, rate: f64, length: Option<u64>| Player {
      rate,
      length: length.map(Duration::from_secs),
      ..player("a", status)
    };
    let five = Duration::from_secs(5);
    assert_eq!(
      at(PlaybackStatus::Playing, 2.0, Some(100)).position_after(five),
      Duration::from_secs(20)
    );
    assert_eq!(
      at(PlaybackStatus::Playing, 0.0, Some(100)).position_after(five),
      Duration::from_secs(10)
    );
    // a negative rate does not run backwards
    assert_eq!(
      at(PlaybackStatus::Playing, -1.0, Some(100)).position_after(five),
      Duration::from_secs(10)
    );
    // no length: no clamp
    assert_eq!(
      at(PlaybackStatus::Playing, 1.0, None).position_after(Duration::from_secs(1000)),
      Duration::from_secs(1010)
    );
    assert_eq!(
      at(PlaybackStatus::Stopped, 1.0, None).position_after(five),
      Duration::from_secs(10)
    );
    // the live position moves on its own
    let playing = Player {
      position_at: Instant::now() - Duration::from_secs(3),
      ..player("a", PlaybackStatus::Playing)
    };
    assert!(playing.position() >= Duration::from_secs(13));
  }

  #[test]
  fn kept_positions_need_the_same_playback() {
    let old = player("a", PlaybackStatus::Playing);
    let fresh = |status, rate, drift_ms: u64| {
      let mut fresh = Player {
        status,
        rate,
        ..old.clone()
      };
      fresh.position_at = old.position_at + Duration::from_secs(2);
      fresh.position = old.position + Duration::from_secs(2) + Duration::from_millis(drift_ms);
      fresh
    };
    let kept = |mut fresh: Player| {
      keep_positions(std::slice::from_mut(&mut fresh), std::slice::from_ref(&old));
      fresh.position == old.position
    };
    assert!(kept(fresh(PlaybackStatus::Playing, 1.0, 500)));
    assert!(!kept(fresh(PlaybackStatus::Playing, 1.0, 501)));
    assert!(!kept(fresh(PlaybackStatus::Playing, 2.0, 0)));
    assert!(!kept(fresh(PlaybackStatus::Paused, 1.0, 0)));
    // backwards within the slack counts too
    let mut back = fresh(PlaybackStatus::Playing, 1.0, 0);
    back.position -= Duration::from_millis(400);
    assert!(kept(back));
    // a player not seen before keeps its read
    let mut new = Player {
      name: "b".into(),
      ..fresh(PlaybackStatus::Playing, 1.0, 0)
    };
    let position = new.position;
    keep_positions(std::slice::from_mut(&mut new), std::slice::from_ref(&old));
    assert_eq!(new.position, position);
  }

  #[test]
  fn active_without_a_current_choice() {
    use PlaybackStatus::*;
    let players = [player("a", Stopped), player("b", Stopped)];
    assert_eq!(pick_active(&players, None).as_deref(), Some("a"));
    assert_eq!(pick_active(&players, Some("b")).as_deref(), Some("b"));
    let players = [player("a", Playing), player("b", Playing)];
    assert_eq!(pick_active(&players, Some("b")).as_deref(), Some("b"));
  }

  #[test]
  fn infinite_rate_in_position_after_panics() {
    let mut p = player("a", PlaybackStatus::Playing);
    p.rate = f64::INFINITY;
    let res = std::panic::catch_unwind(|| {
      p.position_after(Duration::from_secs(1));
    });
    assert!(res.is_err());
  }

  #[test]
  fn position_addition_overflow_panics_without_length() {
    let mut p = player("a", PlaybackStatus::Playing);
    p.length = None;
    p.position = Duration::MAX - Duration::from_secs(10);
    let res = std::panic::catch_unwind(|| {
      p.position_after(Duration::from_secs(20));
    });
    assert!(res.is_err());
  }

  #[test]
  fn paused_player_scrub_under_slack_is_reverted() {
    let old = player("a", PlaybackStatus::Paused);
    let mut fresh = old.clone();
    fresh.position_at = old.position_at + Duration::from_millis(100);
    // Scrub by 200ms while paused (within 500ms SLACK)
    fresh.position = old.position + Duration::from_millis(200);
    keep_positions(std::slice::from_mut(&mut fresh), std::slice::from_ref(&old));
    // Overwritten back to old.position because 200ms <= SLACK (500ms)
    assert_eq!(fresh.position, old.position);

    // Scrub by 600ms while paused (> 500ms SLACK)
    let mut fresh_large = old.clone();
    fresh_large.position_at = old.position_at + Duration::from_millis(100);
    fresh_large.position = old.position + Duration::from_millis(600);
    keep_positions(
      std::slice::from_mut(&mut fresh_large),
      std::slice::from_ref(&old),
    );
    // Kept because 600ms > SLACK
    assert_eq!(
      fresh_large.position,
      old.position + Duration::from_millis(600)
    );
  }
}
