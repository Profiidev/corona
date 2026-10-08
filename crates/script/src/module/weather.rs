use std::time::UNIX_EPOCH;

use corona_weather::{self as wt, WeatherExt, WeatherService};
use gpui_kit::App;
use gpui_shell::HostModule;
use serde::Serialize;
use ts_rs::TS;

use crate::{
  host_fn::{Glob, Module},
  module::{Subscribe, Subscriptions, read},
};
use corona_macros::named;

#[derive(Serialize, TS)]
#[serde(rename_all = "snake_case")]
enum Units {
  /// °C, km/h
  Metric,
  /// °F, mph
  Imperial,
}

#[derive(Serialize, TS)]
struct Location {
  /// Empty for the device's own position.
  name: String,
  latitude: f64,
  longitude: f64,
}

#[derive(Serialize, TS)]
struct Current {
  /// Local time, like `2026-10-08T09:00`.
  time: String,
  temperature: f64,
  apparent_temperature: f64,
  /// In percent.
  humidity: f64,
  wind_speed: f64,
  /// In degrees, 0 for north.
  wind_direction: f64,
  uv_index: f64,
  /// Like `partly_cloudy`, see `Condition`.
  condition: String,
  is_day: bool,
}

#[derive(Serialize, TS)]
struct Hour {
  /// Local time, like `2026-10-08T09:00`.
  time: String,
  temperature: f64,
  /// In percent.
  humidity: f64,
  /// In percent.
  precipitation_probability: f64,
  wind_speed: f64,
  condition: String,
  is_day: bool,
}

#[derive(Serialize, TS)]
struct Day {
  /// Like `2026-10-08`.
  date: String,
  max: f64,
  min: f64,
  condition: String,
  /// Local time, like `2026-10-08T07:12`.
  sunrise: String,
  sunset: String,
}

#[derive(Serialize, TS)]
struct Weather {
  location: Location,
  units: Units,
  /// In meters.
  elevation: f64,
  /// IANA name, like `Europe/Berlin`.
  timezone: String,
  timezone_abbreviation: String,
  current: Current,
  hourly: Vec<Hour>,
  daily: Vec<Day>,
  /// Unix time in seconds.
  fetched: f64,
}

impl From<&wt::Weather> for Weather {
  fn from(w: &wt::Weather) -> Self {
    let c = &w.current;
    Self {
      location: Location {
        name: w.location.name.clone(),
        latitude: w.location.latitude,
        longitude: w.location.longitude,
      },
      units: match w.units {
        wt::Units::Metric => Units::Metric,
        wt::Units::Imperial => Units::Imperial,
      },
      elevation: w.elevation,
      timezone: w.timezone.clone(),
      timezone_abbreviation: w.timezone_abbreviation.clone(),
      current: Current {
        time: c.time.clone(),
        temperature: c.temperature,
        apparent_temperature: c.apparent_temperature,
        humidity: c.humidity,
        wind_speed: c.wind_speed,
        wind_direction: c.wind_direction,
        uv_index: c.uv_index,
        condition: c.condition().key().into(),
        is_day: c.is_day,
      },
      hourly: w
        .hourly
        .iter()
        .map(|h| Hour {
          time: h.time.clone(),
          temperature: h.temperature,
          humidity: h.humidity,
          precipitation_probability: h.precipitation_probability,
          wind_speed: h.wind_speed,
          condition: h.condition().key().into(),
          is_day: h.is_day,
        })
        .collect(),
      daily: w
        .daily
        .iter()
        .map(|d| Day {
          date: d.date.clone(),
          max: d.max,
          min: d.min,
          condition: d.condition().key().into(),
          sunrise: d.sunrise.clone(),
          sunset: d.sunset.clone(),
        })
        .collect(),
      fetched: w
        .fetched
        .duration_since(UNIX_EPOCH)
        .map_or(0., |d| d.as_secs_f64()),
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Updates {
  Weather,
  Error,
}

impl From<Updates> for super::Updates {
  fn from(value: Updates) -> Self {
    super::Updates::Weather(value)
  }
}

pub fn module(reads: &Subscriptions, subs: &mut Vec<Subscribe>, cx: &mut App) -> HostModule {
  let service = cx.weather();

  Module::new("corona/weather")
    .func(read(
      reads,
      subs,
      "current",
      Updates::Weather,
      service.weather.clone(),
      // null until the first forecast arrives
      |cx| cx.weather().current(cx).map(Weather::from),
    ))
    .func(read(
      reads,
      subs,
      "error",
      Updates::Error,
      service.error.clone(),
      |cx| cx.weather().error(cx).map(str::to_string),
    ))
    .func(named!(
      "refresh",
      /// Fetches the forecast again now.
      |weather: Glob<WeatherService>| weather.refresh()
    ))
    .into()
}

#[cfg(test)]
mod tests {
  use std::time::{Duration, UNIX_EPOCH};

  use corona_weather as wt;

  use super::Weather;

  #[test]
  fn converts_hourly_imperial() {
    let mut weather = sample();
    weather.units = wt::Units::Imperial;
    weather.fetched = UNIX_EPOCH - Duration::from_secs(1);
    weather.hourly = vec![wt::Hour {
      time: "2026-10-08T10:00".into(),
      temperature: 54.,
      humidity: 70.,
      precipitation_probability: 20.,
      wind_speed: 5.,
      code: 0,
      is_day: false,
    }];
    let json = serde_json::to_value(Weather::from(&weather)).unwrap();
    assert_eq!(json["units"], "imperial");
    // before the epoch is clamped, not negative
    assert_eq!(json["fetched"], 0.);
    let hour = &json["hourly"][0];
    assert_eq!(hour["time"], "2026-10-08T10:00");
    assert_eq!(hour["precipitation_probability"], 20.);
    assert_eq!(hour["condition"], "clear");
    assert_eq!(hour["is_day"], false);
  }

  #[test]
  fn converts() {
    let json = serde_json::to_value(Weather::from(&sample())).unwrap();
    assert_eq!(json["location"]["name"], "");
    assert_eq!(json["units"], "metric");
    assert_eq!(json["current"]["condition"], "partly_cloudy");
    assert_eq!(json["daily"][0]["condition"], "rain");
    assert_eq!(json["fetched"], 100.);
  }

  fn sample() -> wt::Weather {
    wt::Weather {
      location: wt::Location {
        name: String::new(),
        latitude: 47.86,
        longitude: 12.01,
        query: None,
      },
      units: wt::Units::Metric,
      elevation: 492.,
      timezone: "Europe/Berlin".into(),
      timezone_abbreviation: "CEST".into(),
      current: wt::Current {
        time: "2026-10-08T09:00".into(),
        temperature: 12.5,
        apparent_temperature: 11.,
        humidity: 80.,
        wind_speed: 9.,
        wind_direction: 250.,
        uv_index: 1.5,
        code: 2,
        is_day: true,
      },
      hourly: vec![],
      daily: vec![wt::Day {
        date: "2026-10-08".into(),
        max: 15.,
        min: 6.,
        code: 61,
        sunrise: "2026-10-08T07:20".into(),
        sunset: "2026-10-08T18:40".into(),
      }],
      fetched: UNIX_EPOCH + Duration::from_secs(100),
    }
  }
}
