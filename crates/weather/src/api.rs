//! Open-Meteo: free, no API key, updated hourly.

use std::time::SystemTime;

use anyhow::{Context, Result, bail};
use corona_config::Units;
use futures_lite::AsyncReadExt;
use gpui_kit::http_client::{AsyncBody, HttpClient};
use serde::{Deserialize, de::DeserializeOwned};

use crate::state::{Current, Day, Hour, Location, Weather};

const FORECAST: &str = "https://api.open-meteo.com/v1/forecast";
const GEOCODING: &str = "https://geocoding-api.open-meteo.com/v1/search";
const DAYS: u32 = 7;
const HOURS: u32 = 24;

async fn get<T: DeserializeOwned>(client: &dyn HttpClient, url: &str) -> Result<T> {
  let mut response = client.get(url, AsyncBody::empty(), true).await?;
  let mut body = String::new();
  response.body_mut().read_to_string(&mut body).await?;
  if !response.status().is_success() {
    bail!("{} answered {}: {}", url, response.status(), body.trim());
  }
  serde_json::from_str(&body).with_context(|| format!("parsing {url}"))
}

pub(crate) async fn geocode(client: &dyn HttpClient, city: &str) -> Result<Location> {
  #[derive(Deserialize)]
  struct Results {
    results: Option<Vec<Place>>,
  }
  #[derive(Deserialize)]
  struct Place {
    name: String,
    latitude: f64,
    longitude: f64,
    country: Option<String>,
  }

  let name = city.split(',').next().unwrap_or(city).trim();
  let url = format!("{GEOCODING}?name={}&count=1&format=json", encode(name));
  let place = get::<Results>(client, &url)
    .await?
    .results
    .and_then(|places| places.into_iter().next())
    .with_context(|| format!("no place named {city}"))?;
  Ok(Location {
    name: match place.country {
      Some(country) => format!("{}, {country}", place.name),
      None => place.name,
    },
    latitude: place.latitude,
    longitude: place.longitude,
    query: Some(city.to_string()),
  })
}

pub(crate) async fn forecast(
  client: &dyn HttpClient,
  location: Location,
  units: Units,
) -> Result<Weather> {
  let units_query = match units {
    Units::Metric => "",
    Units::Imperial => "&temperature_unit=fahrenheit&wind_speed_unit=mph",
  };
  let url = format!(
    "{FORECAST}?latitude={}&longitude={}\
     &current=temperature_2m,apparent_temperature,wind_speed_10m,wind_direction_10m,weather_code,is_day,uv_index,relative_humidity_2m\
     &hourly=temperature_2m,relative_humidity_2m,precipitation_probability,weather_code,is_day,wind_speed_10m\
     &daily=temperature_2m_max,temperature_2m_min,weather_code,sunrise,sunset\
     &forecast_days={DAYS}&forecast_hours={HOURS}&timezone=auto{units_query}",
    location.latitude, location.longitude
  );
  let response: Forecast = get(client, &url).await?;
  Ok(response.into_weather(location, units))
}

fn encode(value: &str) -> String {
  value
    .bytes()
    .map(|b| match b {
      b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
        (b as char).to_string()
      }
      _ => format!("%{b:02X}"),
    })
    .collect()
}

#[derive(Deserialize)]
struct Forecast {
  elevation: Option<f64>,
  timezone: Option<String>,
  timezone_abbreviation: Option<String>,
  current: CurrentResponse,
  hourly: HourlyResponse,
  daily: DailyResponse,
}

#[derive(Deserialize)]
struct CurrentResponse {
  time: String,
  temperature_2m: f64,
  apparent_temperature: f64,
  relative_humidity_2m: f64,
  wind_speed_10m: f64,
  wind_direction_10m: f64,
  uv_index: Option<f64>,
  weather_code: u8,
  is_day: u8,
}

#[derive(Deserialize)]
struct HourlyResponse {
  time: Vec<String>,
  temperature_2m: Vec<f64>,
  relative_humidity_2m: Vec<f64>,
  precipitation_probability: Vec<Option<f64>>,
  wind_speed_10m: Vec<f64>,
  weather_code: Vec<u8>,
  is_day: Vec<u8>,
}

#[derive(Deserialize)]
struct DailyResponse {
  time: Vec<String>,
  temperature_2m_max: Vec<f64>,
  temperature_2m_min: Vec<f64>,
  weather_code: Vec<u8>,
  sunrise: Vec<String>,
  sunset: Vec<String>,
}

impl Forecast {
  fn into_weather(self, location: Location, units: Units) -> Weather {
    let c = self.current;
    let h = self.hourly;
    let d = self.daily;
    Weather {
      location,
      units,
      elevation: self.elevation.unwrap_or_default(),
      timezone: self.timezone.unwrap_or_default(),
      timezone_abbreviation: self.timezone_abbreviation.unwrap_or_default(),
      current: Current {
        time: c.time,
        temperature: c.temperature_2m,
        apparent_temperature: c.apparent_temperature,
        humidity: c.relative_humidity_2m,
        wind_speed: c.wind_speed_10m,
        wind_direction: c.wind_direction_10m,
        uv_index: c.uv_index.unwrap_or_default(),
        code: c.weather_code,
        is_day: c.is_day == 1,
      },
      hourly: (0..h.time.len())
        .map_while(|i| {
          Some(Hour {
            time: h.time.get(i)?.clone(),
            temperature: *h.temperature_2m.get(i)?,
            humidity: *h.relative_humidity_2m.get(i)?,
            precipitation_probability: h.precipitation_probability.get(i)?.unwrap_or_default(),
            wind_speed: *h.wind_speed_10m.get(i)?,
            code: *h.weather_code.get(i)?,
            is_day: *h.is_day.get(i)? == 1,
          })
        })
        .collect(),
      daily: (0..d.time.len())
        .map_while(|i| {
          Some(Day {
            date: d.time.get(i)?.clone(),
            max: *d.temperature_2m_max.get(i)?,
            min: *d.temperature_2m_min.get(i)?,
            code: *d.weather_code.get(i)?,
            sunrise: d.sunrise.get(i)?.clone(),
            sunset: d.sunset.get(i)?.clone(),
          })
        })
        .collect(),
      fetched: SystemTime::now(),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const FORECAST_JSON: &str = r#"{
    "latitude": 47.86, "longitude": 12.0, "timezone": "Europe/Berlin",
    "timezone_abbreviation": "GMT+2", "elevation": 482.0,
    "current": {"time": "2026-10-01T14:15", "interval": 900, "temperature_2m": 14.2,
      "apparent_temperature": 12.9, "wind_speed_10m": 7.4, "wind_direction_10m": 250,
      "weather_code": 3, "is_day": 1, "uv_index": 2.1, "relative_humidity_2m": 71},
    "hourly": {"time": ["2026-10-01T14:00", "2026-10-01T15:00"],
      "temperature_2m": [14.0, 14.4], "relative_humidity_2m": [72, 70],
      "precipitation_probability": [10, null], "weather_code": [3, 61],
      "is_day": [1, 1], "wind_speed_10m": [7.0, 8.1]},
    "daily": {"time": ["2026-10-01"], "temperature_2m_max": [16.3],
      "temperature_2m_min": [8.1], "weather_code": [61],
      "sunrise": ["2026-10-01T07:12"], "sunset": ["2026-10-01T18:54"]}
  }"#;

  #[test]
  fn forecast() {
    let location = Location {
      name: "Bad Aibling, Germany".into(),
      latitude: 47.86,
      longitude: 12.0,
      query: None,
    };
    let forecast: Forecast = serde_json::from_str(FORECAST_JSON).unwrap();
    let weather = forecast.into_weather(location, Units::Metric);
    assert_eq!(weather.current.temperature, 14.2);
    assert!(weather.current.is_day);
    assert_eq!(weather.hourly.len(), 2);
    // a missing probability reads as 0
    assert_eq!(weather.hourly[1].precipitation_probability, 0.);
    assert_eq!(weather.daily[0].sunset, "2026-10-01T18:54");
    assert_eq!(
      (weather.elevation, weather.timezone_abbreviation.as_str()),
      (482., "GMT+2")
    );
  }

  #[test]
  fn encode() {
    assert_eq!(super::encode("Bad Aibling"), "Bad%20Aibling");
    assert_eq!(super::encode("München"), "M%C3%BCnchen");
  }
}

#[cfg(test)]
mod live {
  use super::*;

  /// asks Open-Meteo for real: `cargo test -p corona_weather -- --ignored --nocapture`
  #[test]
  #[ignore]
  fn bad_aibling() {
    let client = corona_reqwest::client().unwrap();
    futures_lite::future::block_on(async {
      let location = geocode(&client, "Bad Aibling, Germany").await.unwrap();
      println!("{location:?}");
      let weather = forecast(&client, location, Units::Metric).await.unwrap();
      println!(
        "{} {}°C ({}) feels {}°C, elevation {}m, {} {}",
        weather.current.time,
        weather.current.temperature,
        weather.current.condition().key(),
        weather.current.apparent_temperature,
        weather.elevation,
        weather.timezone,
        weather.timezone_abbreviation
      );
      for day in &weather.daily {
        println!(
          "{} {}/{} {}",
          day.date,
          day.min,
          day.max,
          day.condition().key()
        );
      }
    });
  }
}
