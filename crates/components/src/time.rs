use std::time::Duration;

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

/// A length of time as days, hours and minutes: "4m", "2h 5m", "1d 0h 3m"
pub fn span(d: Duration) -> String {
  let minutes = d.as_secs() / 60;
  let (days, hours, minutes) = (minutes / 1440, minutes / 60 % 24, minutes % 60);
  match (days, hours) {
    (0, 0) => format!("{minutes}m"),
    (0, _) => format!("{hours}h {minutes}m"),
    _ => format!("{days}d {hours}h {minutes}m"),
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
}
