use std::time::SystemTime;

use corona_config::Units;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Location {
  /// Empty for the device's own position, which the shell names
  pub name: String,
  pub latitude: f64,
  pub longitude: f64,
  pub query: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Condition {
  Clear,
  MainlyClear,
  PartlyCloudy,
  Overcast,
  Fog,
  Drizzle,
  FreezingDrizzle,
  Rain,
  FreezingRain,
  Snow,
  SnowGrains,
  RainShowers,
  SnowShowers,
  Thunderstorm,
  ThunderstormHail,
  Unknown,
}

impl Condition {
  pub fn from_code(code: u8) -> Self {
    match code {
      0 => Self::Clear,
      1 => Self::MainlyClear,
      2 => Self::PartlyCloudy,
      3 => Self::Overcast,
      45 | 48 => Self::Fog,
      51 | 53 | 55 => Self::Drizzle,
      56 | 57 => Self::FreezingDrizzle,
      61 | 63 | 65 => Self::Rain,
      66 | 67 => Self::FreezingRain,
      71 | 73 | 75 => Self::Snow,
      77 => Self::SnowGrains,
      80..=82 => Self::RainShowers,
      85 | 86 => Self::SnowShowers,
      95 => Self::Thunderstorm,
      96 | 99 => Self::ThunderstormHail,
      _ => Self::Unknown,
    }
  }

  pub const ALL: [Self; 16] = [
    Self::Clear,
    Self::MainlyClear,
    Self::PartlyCloudy,
    Self::Overcast,
    Self::Fog,
    Self::Drizzle,
    Self::FreezingDrizzle,
    Self::Rain,
    Self::FreezingRain,
    Self::Snow,
    Self::SnowGrains,
    Self::RainShowers,
    Self::SnowShowers,
    Self::Thunderstorm,
    Self::ThunderstormHail,
    Self::Unknown,
  ];

  /// Translation key, like `partly_cloudy`
  pub fn key(self) -> &'static str {
    match self {
      Self::Clear => "clear",
      Self::MainlyClear => "mainly_clear",
      Self::PartlyCloudy => "partly_cloudy",
      Self::Overcast => "overcast",
      Self::Fog => "fog",
      Self::Drizzle => "drizzle",
      Self::FreezingDrizzle => "freezing_drizzle",
      Self::Rain => "rain",
      Self::FreezingRain => "freezing_rain",
      Self::Snow => "snow",
      Self::SnowGrains => "snow_grains",
      Self::RainShowers => "rain_showers",
      Self::SnowShowers => "snow_showers",
      Self::Thunderstorm => "thunderstorm",
      Self::ThunderstormHail => "thunderstorm_hail",
      Self::Unknown => "unknown",
    }
  }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Current {
  pub time: String,
  pub temperature: f64,
  pub apparent_temperature: f64,
  pub humidity: f64,
  pub wind_speed: f64,
  pub wind_direction: f64,
  pub uv_index: f64,
  pub code: u8,
  pub is_day: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hour {
  pub time: String,
  pub temperature: f64,
  pub humidity: f64,
  pub precipitation_probability: f64,
  pub wind_speed: f64,
  pub code: u8,
  pub is_day: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Day {
  pub date: String,
  pub max: f64,
  pub min: f64,
  pub code: u8,
  pub sunrise: String,
  pub sunset: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Weather {
  pub location: Location,
  pub units: Units,
  pub elevation: f64,
  pub timezone: String,
  pub timezone_abbreviation: String,
  pub current: Current,
  pub hourly: Vec<Hour>,
  pub daily: Vec<Day>,
  pub fetched: SystemTime,
}

/// Compass point as translation key, like `ne`
pub fn compass(degrees: f64) -> &'static str {
  const POINTS: [&str; 8] = ["n", "ne", "e", "se", "s", "sw", "w", "nw"];
  POINTS[((degrees.rem_euclid(360.) + 22.5) / 45.) as usize % 8]
}

impl Current {
  pub fn condition(&self) -> Condition {
    Condition::from_code(self.code)
  }
}

impl Hour {
  pub fn condition(&self) -> Condition {
    Condition::from_code(self.code)
  }
}

impl Day {
  pub fn condition(&self) -> Condition {
    Condition::from_code(self.code)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn calendar() {
    assert_eq!(compass(250.), "w");
    assert_eq!(compass(359.), "n");
    assert_eq!(compass(135.), "se");
  }

  #[test]
  fn conditions() {
    assert_eq!(Condition::from_code(0), Condition::Clear);
    assert_eq!(Condition::from_code(81), Condition::RainShowers);
    assert_eq!(Condition::from_code(99), Condition::ThunderstormHail);
    assert_eq!(Condition::from_code(42), Condition::Unknown);
    assert_eq!(Condition::PartlyCloudy.key(), "partly_cloudy");
  }

  #[test]
  fn keys_are_unique() {
    let mut keys: Vec<_> = Condition::ALL.iter().map(|c| c.key()).collect();
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), Condition::ALL.len());
  }
}
