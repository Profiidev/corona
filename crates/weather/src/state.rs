use std::time::SystemTime;

use corona_config::Units;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Location {
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

  pub fn description(self) -> &'static str {
    match self {
      Self::Clear => "Clear sky",
      Self::MainlyClear => "Mainly clear",
      Self::PartlyCloudy => "Partly cloudy",
      Self::Overcast => "Overcast",
      Self::Fog => "Fog",
      Self::Drizzle => "Drizzle",
      Self::FreezingDrizzle => "Freezing drizzle",
      Self::Rain => "Rain",
      Self::FreezingRain => "Freezing rain",
      Self::Snow => "Snow",
      Self::SnowGrains => "Snow grains",
      Self::RainShowers => "Rain showers",
      Self::SnowShowers => "Snow showers",
      Self::Thunderstorm => "Thunderstorm",
      Self::ThunderstormHail => "Thunderstorm with hail",
      Self::Unknown => "Unknown",
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

pub fn weekday(date: &str) -> Option<&'static str> {
  const NAMES: [&str; 7] = [
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
  ];
  let mut parts = date.get(..10)?.split('-').map(|p| p.parse::<i64>().ok());
  let (y, m, d) = (parts.next()??, parts.next()??, parts.next()??);
  let y = if m <= 2 { y - 1 } else { y };
  let era = y.div_euclid(400);
  let yoe = y - era * 400;
  let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
  let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
  let days = era * 146097 + doe - 719468;
  Some(NAMES[days.rem_euclid(7) as usize])
}

pub fn compass(degrees: f64) -> &'static str {
  const POINTS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
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
    assert_eq!(weekday("2026-10-01"), Some("Thursday"));
    assert_eq!(weekday("2024-02-29"), Some("Thursday"));
    assert_eq!(weekday("2000-01-01"), Some("Saturday"));
    assert_eq!(weekday("garbage"), None);
    assert_eq!(compass(250.), "W");
    assert_eq!(compass(359.), "N");
    assert_eq!(compass(135.), "SE");
  }

  #[test]
  fn conditions() {
    assert_eq!(Condition::from_code(0), Condition::Clear);
    assert_eq!(Condition::from_code(81), Condition::RainShowers);
    assert_eq!(Condition::from_code(99), Condition::ThunderstormHail);
    assert_eq!(Condition::from_code(42), Condition::Unknown);
  }
}
