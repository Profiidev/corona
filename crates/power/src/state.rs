use std::time::Duration;

pub use upower_dbus::{BatteryLevel, BatteryState, BatteryType};
use zbus::zvariant::OwnedObjectPath;

#[derive(Clone, Debug, PartialEq)]
pub struct Battery {
  pub percentage: f64,
  pub state: BatteryState,
  pub time_to_empty: Option<Duration>,
  pub time_to_full: Option<Duration>,
  pub energy_rate: f64,
  pub energy: f64,
  pub energy_full: f64,
  pub energy_full_design: f64,
  pub capacity: f64,
  pub charge_threshold: Option<ChargeThreshold>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChargeThreshold {
  pub(crate) battery: OwnedObjectPath,
  pub enabled: bool,
  pub start: u32,
  pub end: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PowerDevice {
  pub path: OwnedObjectPath,
  pub model: String,
  pub kind: BatteryType,
  pub percentage: f64,
  pub state: BatteryState,
  pub level: BatteryLevel,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Status {
  pub on_battery: bool,
  pub lid_closed: Option<bool>,
  pub critical_action: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Profiles {
  /// `power-saver`, `balanced` or `performance`
  pub active: String,
  pub available: Vec<String>,
  pub degraded: Option<String>,
}

impl Profiles {
  /// The one after the active profile, wrapping around
  pub fn next(&self) -> Option<&String> {
    let at = self.available.iter().position(|a| *a == self.active)?;
    self.available.get((at + 1) % self.available.len())
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyboardBacklight {
  pub brightness: i32,
  pub max: i32,
}

/// UPower reports unknown durations as 0
pub(crate) fn duration(seconds: i64) -> Option<Duration> {
  (seconds > 0).then(|| Duration::from_secs(seconds as u64))
}

/// the system battery and line power are not peripherals
pub(crate) fn is_peripheral(kind: BatteryType, power_supply: bool) -> bool {
  !power_supply && !matches!(kind, BatteryType::LinePower | BatteryType::Unknown)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn helpers() {
    assert_eq!(duration(0), None);
    assert_eq!(duration(90), Some(Duration::from_secs(90)));
    // a laptop battery powers the system, a mouse battery does not
    assert!(!is_peripheral(BatteryType::Battery, true));
    assert!(is_peripheral(BatteryType::Mouse, false));
    assert!(!is_peripheral(BatteryType::LinePower, false));
  }

  fn profiles(active: &str, available: &[&str]) -> Profiles {
    Profiles {
      active: active.into(),
      available: available.iter().map(|a| a.to_string()).collect(),
      degraded: None,
    }
  }

  #[test]
  fn next_profile() {
    let all = ["power-saver", "balanced", "performance"];
    assert_eq!(
      profiles("power-saver", &all).next().map(String::as_str),
      Some("balanced")
    );
    assert_eq!(
      profiles("performance", &all).next().map(String::as_str),
      Some("power-saver")
    );
    assert_eq!(
      profiles("balanced", &["balanced"])
        .next()
        .map(String::as_str),
      Some("balanced")
    );
    assert_eq!(profiles("custom", &all).next(), None);
    assert_eq!(profiles("balanced", &[]).next(), None);
  }

  #[test]
  fn helper_edges() {
    assert_eq!(duration(-5), None);
    assert_eq!(
      duration(i64::MAX),
      Some(Duration::from_secs(i64::MAX as u64))
    );
    assert!(!is_peripheral(BatteryType::Unknown, false));
    assert!(!is_peripheral(BatteryType::Mouse, true));
    for kind in [
      BatteryType::Battery,
      BatteryType::Ups,
      BatteryType::Headset,
      BatteryType::BluetoothGeneric,
    ] {
      assert!(is_peripheral(kind, false), "{kind:?}");
    }
  }
}
