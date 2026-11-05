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

/// A remaining or elapsed time, localized and rounded to the minute: "25 min", "1 h 5 min"
pub fn duration(d: Duration) -> String {
  let minutes = (d.as_secs() + 30) / 60;
  match minutes / 60 {
    0 => t!("app.power.duration.minutes", minutes = minutes).into(),
    hours => t!(
      "app.power.duration.hours",
      hours = hours,
      minutes = minutes % 60
    )
    .into(),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn localized_duration() {
    let d = |s| duration(Duration::from_secs(s));
    assert_eq!(d(0), "0 min");
    assert_eq!(d(25 * 60), "25 min");
    assert_eq!(d(29), "0 min");
    assert_eq!(d(30), "1 min");
    assert_eq!(d(3599), "1 h 0 min");
    assert_eq!(d(65 * 60), "1 h 5 min");
    assert_eq!(d(4 * 3600 + 20 * 60), "4 h 20 min");
    // no day unit, hours keep counting
    assert_eq!(d(26 * 3600 + 3 * 60), "26 h 3 min");
  }

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
