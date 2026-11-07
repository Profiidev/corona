use std::time::UNIX_EPOCH;

use crate as wt;
use corona_weather::{WeatherExt, WeatherService};
use corona_js::{Glob, Json, ModuleBuilder, PluginRef, ScriptModule, host_module};
use gpui_kit::App;
use serde::Serialize;
use specta::{Type, specta};

#[derive(Serialize, Type)]
#[serde(rename_all = "snake_case")]
enum Units {
  /// °C, km/h
  Metric,
  /// °F, mph
  Imperial,
}

#[derive(Serialize, Type)]
struct Location {
  /// Empty for the device's own position.
  name: String,
  /// Rounded to 0.1°, about 11 km, so plugins do not learn where the user is.
  latitude: f64,
  longitude: f64,
}

fn coarse(degrees: f64) -> f64 {
  (degrees * 10.).round() / 10.
}

#[derive(Serialize, Type)]
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

#[derive(Serialize, Type)]
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

#[derive(Serialize, Type)]
struct Day {
  /// Like `2026-10-08`.
  date: String,
  max: f64,
  min: f64,
  condition: String,
  /// Local time, like `2026-10-08T07:12`.
  sunrise: Option<String>,
  sunset: Option<String>,
}

#[derive(Serialize, Type)]
#[serde(rename = "Weather")]
struct Forecast {
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

impl From<&wt::Weather> for Forecast {
  fn from(w: &wt::Weather) -> Self {
    let c = &w.current;
    Self {
      location: Location {
        name: w.location.name.clone(),
        latitude: coarse(w.location.latitude),
        longitude: coarse(w.location.longitude),
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

/// Fetches the forecast again now.
#[specta]
fn refresh(weather: Glob<WeatherService>) {
  weather.refresh()
}

/// `corona/weather`
pub struct WeatherModule;

impl ScriptModule for WeatherModule {
  const NAME: &str = "weather";
  const DOC: &str = "The weather forecast where the user is.";

  fn module(_: &PluginRef, m: &mut ModuleBuilder, _: &mut App) {
    m.watch(
      "current",
      "The forecast; null until the first one arrives.",
      |cx| cx.weather().weather.clone(),
      |cx| Json(cx.weather().current(cx).map(Forecast::from)),
    );
    m.watch(
      "error",
      "Why the last fetch failed; null when it did not.",
      |cx| cx.weather().error.clone(),
      |cx| Json(cx.weather().error(cx).map(str::to_string)),
    );
    host_module!(m, { sync: [refresh] });
  }
}

#[cfg(test)]
mod tests {
  use std::time::{Duration, UNIX_EPOCH};

  use crate as wt;

  use super::Forecast;

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
    let json = serde_json::to_value(Forecast::from(&weather)).unwrap();
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
    let json = serde_json::to_value(Forecast::from(&sample())).unwrap();
    assert_eq!(json["location"]["name"], "");
    assert_eq!(json["location"]["latitude"], 47.9);
    assert_eq!(json["location"]["longitude"], 12.);
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
        sunrise: Some("2026-10-08T07:20".into()),
        sunset: Some("2026-10-08T18:40".into()),
      }],
      fetched: UNIX_EPOCH + Duration::from_secs(100),
    }
  }
}
