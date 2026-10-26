//! Sunrise and sunset by the NOAA solar calculator equations, good to a minute or so.

use jiff::{Timestamp, civil::Date};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Day {
  Rises {
    rise: Timestamp,
    set: Timestamp,
  },
  /// The sun stays below the horizon all day
  PolarNight,
  /// The sun stays above the horizon all day
  MidnightSun,
}

/// Julian centuries since J2000.0
fn centuries(julian_day: f64) -> f64 {
  (julian_day - 2451545.) / 36525.
}

/// (equation of time in minutes, declination in radians)
fn position(t: f64) -> (f64, f64) {
  let mean_longitude = (280.46646 + t * (36000.76983 + t * 0.0003032)).rem_euclid(360.);
  let mean_anomaly = 357.52911 + t * (35999.05029 - 0.0001537 * t);
  let eccentricity = 0.016708634 - t * (0.000042037 + 0.0000001267 * t);
  let m = mean_anomaly.to_radians();
  let center = m.sin() * (1.914602 - t * (0.004817 + 0.000014 * t))
    + (2. * m).sin() * (0.019993 - 0.000101 * t)
    + (3. * m).sin() * 0.000289;
  let omega = (125.04 - 1934.136 * t).to_radians();
  let apparent_longitude = (mean_longitude + center - 0.00569 - 0.00478 * omega.sin()).to_radians();
  let mean_obliquity =
    23. + (26. + (21.448 - t * (46.815 + t * (0.00059 - t * 0.001813))) / 60.) / 60.;
  let obliquity = (mean_obliquity + 0.00256 * omega.cos()).to_radians();
  let declination = (obliquity.sin() * apparent_longitude.sin()).asin();

  let y = (obliquity / 2.).tan().powi(2);
  let l = mean_longitude.to_radians();
  let e = eccentricity;
  let equation = y * (2. * l).sin() - 2. * e * m.sin() + 4. * e * y * m.sin() * (2. * l).cos()
    - 0.5 * y * y * (4. * l).sin()
    - 1.25 * e * e * (2. * m).sin();
  (4. * equation.to_degrees(), declination)
}

/// Minutes after midnight UTC of sunrise (`rising`) or sunset, None when there is none
fn event(julian_midnight: f64, latitude: f64, longitude: f64, rising: bool) -> Option<f64> {
  let sign = if rising { 1. } else { -1. };
  // first guess at local noon, then once more at the event itself
  let mut minutes = 720. - 4. * longitude;
  for _ in 0..2 {
    let (equation, declination) = position(centuries(julian_midnight + minutes / 1440.));
    let lat = latitude.to_radians();
    // the sun's upper limb at the horizon, with refraction
    let cos_hour = 90.833f64.to_radians().cos() / (lat.cos() * declination.cos())
      - lat.tan() * declination.tan();
    if !(-1. ..=1.).contains(&cos_hour) {
      return None;
    }
    let hour = cos_hour.acos().to_degrees();
    minutes = 720. - 4. * (longitude + sign * hour) - equation;
  }
  Some(minutes)
}

/// The sun on the UTC day `date` at `latitude` (north) and `longitude` (east)
pub(crate) fn sun(date: Date, latitude: f64, longitude: f64) -> Day {
  let midnight = date.to_zoned(jiff::tz::TimeZone::UTC).unwrap().timestamp();
  let julian_midnight = midnight.as_second() as f64 / 86400. + 2440587.5;
  let at = |minutes: f64| midnight + jiff::SignedDuration::from_secs_f64(minutes * 60.);
  match (
    event(julian_midnight, latitude, longitude, true),
    event(julian_midnight, latitude, longitude, false),
  ) {
    (Some(rise), Some(set)) => Day::Rises {
      rise: at(rise),
      set: at(set),
    },
    _ => {
      // no event: the noon declination says whether the sun is up or down all day
      let (_, declination) = position(centuries(julian_midnight + 0.5 - longitude / 360.));
      let noon_elevation = 90. - (latitude - declination.to_degrees()).abs();
      if noon_elevation > -0.833 {
        Day::MidnightSun
      } else {
        Day::PolarNight
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use jiff::{civil::date, tz::TimeZone};

  use super::*;

  /// local `HH:MM` in `zone`
  fn local(at: Timestamp, zone: &str) -> String {
    at.to_zoned(TimeZone::get(zone).unwrap())
      .strftime("%H:%M")
      .to_string()
  }

  fn minutes(text: &str) -> i32 {
    let (h, m) = text.split_once(':').unwrap();
    h.parse::<i32>().unwrap() * 60 + m.parse::<i32>().unwrap()
  }

  /// within two minutes of the NOAA calculator
  fn near(at: Timestamp, zone: &str, want: &str) {
    let got = local(at, zone);
    assert!(
      (minutes(&got) - minutes(want)).abs() <= 2,
      "{got} is not {want}"
    );
  }

  fn rises(sun: Day) -> (Timestamp, Timestamp) {
    match sun {
      Day::Rises { rise, set } => (rise, set),
      other => panic!("{other:?}"),
    }
  }

  #[test]
  fn berlin_solstices() {
    let (rise, set) = rises(sun(date(2026, 6, 21), 52.52, 13.405));
    near(rise, "Europe/Berlin", "04:43");
    near(set, "Europe/Berlin", "21:33");
    let (rise, set) = rises(sun(date(2026, 12, 21), 52.52, 13.405));
    near(rise, "Europe/Berlin", "08:15");
    near(set, "Europe/Berlin", "15:54");
  }

  #[test]
  fn southern_and_western() {
    // Sydney in its summer
    let (rise, set) = rises(sun(date(2026, 12, 21), -33.87, 151.21));
    near(rise, "Australia/Sydney", "05:41");
    near(set, "Australia/Sydney", "20:05");
    // New York, west of Greenwich: sunset falls on the next UTC day
    let (rise, set) = rises(sun(date(2026, 3, 20), 40.71, -74.01));
    near(rise, "America/New_York", "06:59");
    near(set, "America/New_York", "19:09");
    assert!(set > rise);
  }

  #[test]
  fn the_poles() {
    // Tromsø
    assert_eq!(sun(date(2026, 12, 21), 69.65, 18.96), Day::PolarNight);
    assert_eq!(sun(date(2026, 6, 21), 69.65, 18.96), Day::MidnightSun);
    // the other way round in the south
    assert_eq!(sun(date(2026, 6, 21), -75., 0.), Day::PolarNight);
    assert_eq!(sun(date(2026, 12, 21), -75., 0.), Day::MidnightSun);
    // the equator has days of about twelve hours all year
    let (rise, set) = rises(sun(date(2026, 6, 21), 0., 0.));
    let length = set.duration_since(rise).as_mins();
    assert!((715..=735).contains(&length), "{length}");
  }
}
