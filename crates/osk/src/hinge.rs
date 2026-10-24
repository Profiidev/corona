//! Tablet mode from the hinge angle sensor of a convertible, read from IIO
//! sysfs. Sysfs has no change events, so it is polled.

use std::{
  fs,
  path::{Path, PathBuf},
};

/// Where the kernel lists IIO sensors
pub const IIO_DEVICES: &str = "/sys/bus/iio/devices";
/// Folded further than this is tablet mode
const TABLET_ABOVE: f64 = 200.;
/// Opened less than this is laptop mode again; in between nothing changes, so
/// wobbling near the edge does not flip it
const LAPTOP_BELOW: f64 = 160.;

/// The hinge angle channel of a sensor
#[derive(Debug, PartialEq)]
pub struct Hinge {
  raw: PathBuf,
  scale: f64,
}

impl Hinge {
  /// The first sensor named like a hinge, and in it the angle channel whose
  /// label says hinge; channel 0 when it labels none
  pub fn find(devices: &Path) -> Option<Self> {
    let mut sensors: Vec<PathBuf> = fs::read_dir(devices)
      .ok()?
      .flatten()
      .map(|d| d.path())
      .filter(|d| read(&d.join("name")).is_some_and(|n| n.contains("hinge")))
      .collect();
    sensors.sort();
    sensors.into_iter().find_map(|sensor| {
      let labelled = (0..8).find(|i| {
        read(&sensor.join(format!("in_angl{i}_label"))).is_some_and(|l| l.contains("hinge"))
      });
      let any_label = (0..8).any(|i| sensor.join(format!("in_angl{i}_label")).exists());
      let channel = match (labelled, any_label) {
        (Some(i), _) => i,
        (None, false) => 0,
        (None, true) => return None,
      };
      let raw = sensor.join(format!("in_angl{channel}_raw"));
      let scale = read(&sensor.join(format!("in_angl{channel}_scale")))
        .and_then(|s| s.parse().ok())
        .unwrap_or(1.);
      raw.exists().then_some(Self { raw, scale })
    })
  }

  /// The angle in degrees, none when the sensor is gone
  pub fn angle(&self) -> Option<f64> {
    let raw: f64 = read(&self.raw)?.parse().ok()?;
    Some(raw * self.scale)
  }
}

/// Tablet mode at `angle`, given whether it was before
pub fn is_tablet(angle: f64, was: bool) -> bool {
  if angle > TABLET_ABOVE {
    true
  } else if angle < LAPTOP_BELOW {
    false
  } else {
    was
  }
}

fn read(path: &Path) -> Option<String> {
  Some(fs::read_to_string(path).ok()?.trim().to_string())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn sensor(devices: &Path, dir: &str, files: &[(&str, &str)]) {
    let dir = devices.join(dir);
    fs::create_dir_all(&dir).unwrap();
    for (name, value) in files {
      fs::write(dir.join(name), format!("{value}\n")).unwrap();
    }
  }

  #[test]
  fn finds_the_labelled_hinge_channel() {
    let devices = tempfile::tempdir().unwrap();
    sensor(devices.path(), "iio:device0", &[("name", "accel_3d")]);
    sensor(
      devices.path(),
      "iio:device6",
      &[
        ("name", "hinge"),
        ("in_angl0_label", "screen"),
        ("in_angl0_raw", "10"),
        ("in_angl1_label", "hinge"),
        ("in_angl1_raw", "2100"),
        ("in_angl1_scale", "0.1"),
      ],
    );
    let hinge = Hinge::find(devices.path()).unwrap();
    assert_eq!(hinge.angle(), Some(210.));
  }

  #[test]
  fn unlabelled_sensors_use_channel_zero() {
    let devices = tempfile::tempdir().unwrap();
    sensor(
      devices.path(),
      "iio:device2",
      &[("name", "hinge"), ("in_angl0_raw", "95")],
    );
    assert_eq!(Hinge::find(devices.path()).unwrap().angle(), Some(95.));
    assert_eq!(Hinge::find(&devices.path().join("missing")), None);
  }

  #[test]
  fn flips_only_past_the_thresholds() {
    assert!(is_tablet(250., false));
    assert!(is_tablet(180., true));
    assert!(!is_tablet(180., false));
    assert!(!is_tablet(120., true));
  }
}
