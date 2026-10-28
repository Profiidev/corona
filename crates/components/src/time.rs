use std::time::{Duration, SystemTime};

use rust_i18n::t;

/// How long ago, short: "now", "4m", "1h", "3d"
pub fn ago_short(elapsed: Duration) -> String {
  match elapsed.as_secs() {
    0..60 => t!("app.notifications.ago.now").into(),
    s @ 60..3600 => t!("app.notifications.ago.minutes", count = s / 60).into(),
    s @ 3600..86400 => t!("app.notifications.ago.hours", count = s / 3600).into(),
    s => t!("app.notifications.ago.days", count = s / 86400).into(),
  }
}

/// How long before `now` `time` was, spelled out: "just now", "5 min ago"
pub fn ago_long(time: SystemTime, now: SystemTime) -> String {
  let secs = now.duration_since(time).unwrap_or_default().as_secs();
  match secs {
    0..60 => t!("app.privacy.ago.now").into(),
    60..3600 => t!("app.privacy.ago.minutes", count = secs / 60).into(),
    3600..86400 => t!("app.privacy.ago.hours", count = secs / 3600).into(),
    _ => t!("app.privacy.ago.days", count = secs / 86400).into(),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn short() {
    assert_eq!(ago_short(Duration::from_secs(5)), "now");
    assert_eq!(ago_short(Duration::from_secs(4 * 60)), "4m");
    assert_eq!(ago_short(Duration::from_secs(90 * 60)), "1h");
    assert_eq!(ago_short(Duration::from_secs(3 * 86400)), "3d");
  }

  #[test]
  fn short_boundaries() {
    let ago = |s| ago_short(Duration::from_secs(s));
    assert_eq!(ago(59), "now");
    assert_eq!(ago(60), "1m");
    assert_eq!(ago(3599), "59m");
    assert_eq!(ago(3600), "1h");
    assert_eq!(ago(86399), "23h");
    assert_eq!(ago(86400), "1d");
  }

  #[test]
  fn long() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100_000);
    let before = |secs| now - Duration::from_secs(secs);
    assert_eq!(ago_long(before(5), now), "just now");
    assert_eq!(ago_long(before(300), now), "5 min ago");
    assert_eq!(ago_long(before(7200), now), "2 h ago");
    assert_eq!(ago_long(before(90_000), now), "1 d ago");
  }
}
