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
    country_code: Option<String>,
    admin1: Option<String>,
  }

  // Open-Meteo searches names only: "Paris, Texas" asks for Paris and picks the one in Texas
  let (name, region) = match city.split_once(',') {
    Some((name, region)) => (name.trim(), Some(region.trim()).filter(|r| !r.is_empty())),
    None => (city.trim(), None),
  };
  let count = if region.is_some() { 10 } else { 1 };
  let url = format!(
    "{GEOCODING}?name={}&count={count}&format=json",
    encode(name)
  );
  let places = get::<Results>(client, &url)
    .await?
    .results
    .unwrap_or_default();
  let in_region = |place: &Place| {
    region.is_some_and(|region| {
      [&place.admin1, &place.country, &place.country_code]
        .into_iter()
        .flatten()
        .any(|part| part.eq_ignore_ascii_case(region))
    })
  };
  let place = match places.iter().position(in_region) {
    Some(at) => places.into_iter().nth(at),
    None => places.into_iter().next(),
  }
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

/// Open-Meteo answers `null` where a model has no value: those hours and days are left out
#[derive(Deserialize)]
struct HourlyResponse {
  time: Vec<String>,
  temperature_2m: Vec<Option<f64>>,
  relative_humidity_2m: Vec<Option<f64>>,
  precipitation_probability: Vec<Option<f64>>,
  wind_speed_10m: Vec<Option<f64>>,
  weather_code: Vec<Option<u8>>,
  is_day: Vec<Option<u8>>,
}

#[derive(Deserialize)]
struct DailyResponse {
  time: Vec<String>,
  temperature_2m_max: Vec<Option<f64>>,
  temperature_2m_min: Vec<Option<f64>>,
  weather_code: Vec<Option<u8>>,
  sunrise: Vec<Option<String>>,
  sunset: Vec<Option<String>>,
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
        .filter_map(|i| {
          Some(Hour {
            time: h.time.get(i)?.clone(),
            temperature: (*h.temperature_2m.get(i)?)?,
            humidity: (*h.relative_humidity_2m.get(i)?)?,
            precipitation_probability: h.precipitation_probability.get(i)?.unwrap_or_default(),
            wind_speed: (*h.wind_speed_10m.get(i)?)?,
            code: (*h.weather_code.get(i)?)?,
            is_day: (*h.is_day.get(i)?)? == 1,
          })
        })
        .collect(),
      daily: (0..d.time.len())
        .filter_map(|i| {
          Some(Day {
            date: d.time.get(i)?.clone(),
            max: (*d.temperature_2m_max.get(i)?)?,
            min: (*d.temperature_2m_min.get(i)?)?,
            code: (*d.weather_code.get(i)?)?,
            sunrise: d.sunrise.get(i)?.clone()?,
            sunset: d.sunset.get(i)?.clone()?,
          })
        })
        .collect(),
      fetched: SystemTime::now(),
    }
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use super::*;

  pub(crate) const FORECAST_JSON: &str = r#"{
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

  use std::sync::{Arc, Mutex};

  use futures_lite::future::block_on;
  use gpui_kit::http_client::{FakeHttpClient, HttpClientWithUrl, Response};

  /// answers every request with `answer(url)` and records the URLs
  pub(crate) fn fake(
    answer: impl Fn(&str) -> (u16, String) + Send + Sync + 'static,
  ) -> (Arc<HttpClientWithUrl>, Arc<Mutex<Vec<String>>>) {
    let urls = Arc::new(Mutex::new(Vec::new()));
    let seen = urls.clone();
    let answer = Arc::new(answer);
    let client = FakeHttpClient::create(move |req| {
      let url = req.uri().to_string();
      seen.lock().unwrap().push(url.clone());
      let (status, body) = answer(&url);
      async move {
        Ok(
          Response::builder()
            .status(status)
            .body(body.into())
            .unwrap(),
        )
      }
    });
    (client, urls)
  }

  fn here() -> Location {
    Location {
      name: "Here".into(),
      latitude: 47.86,
      longitude: -12.5,
      query: None,
    }
  }

  fn parse(json: &str) -> Weather {
    serde_json::from_str::<Forecast>(json)
      .unwrap()
      .into_weather(here(), Units::Metric)
  }

  #[test]
  fn encode_edges() {
    assert_eq!(super::encode(""), "");
    assert_eq!(super::encode("AZaz09-_.~"), "AZaz09-_.~");
    assert_eq!(
      super::encode("a+b,c&d=e/f?#%"),
      "a%2Bb%2Cc%26d%3De%2Ff%3F%23%25"
    );
    assert_eq!(super::encode("東京"), "%E6%9D%B1%E4%BA%AC");
  }

  #[test]
  fn ragged_arrays_are_cut_to_the_shortest() {
    let json = FORECAST_JSON
      .replace(
        r#""wind_speed_10m": [7.0, 8.1]"#,
        r#""wind_speed_10m": [7.0]"#,
      )
      .replace(r#""sunset": ["2026-10-01T18:54"]"#, r#""sunset": []"#);
    let weather = parse(&json);
    assert_eq!(weather.hourly.len(), 1);
    assert!(weather.daily.is_empty());
  }

  #[test]
  fn optional_fields_default() {
    let json = FORECAST_JSON
      .replace(r#""timezone": "Europe/Berlin","#, "")
      .replace(
        r#""timezone_abbreviation": "GMT+2", "elevation": 482.0,"#,
        "",
      )
      .replace(r#", "uv_index": 2.1"#, "");
    let weather = parse(&json);
    assert_eq!((weather.elevation, weather.timezone.as_str()), (0., ""));
    assert_eq!(weather.timezone_abbreviation, "");
    assert_eq!(weather.current.uv_index, 0.);
    assert_eq!(weather.location, here());
    assert_eq!(weather.units, Units::Metric);
  }

  #[test]
  fn only_one_is_day() {
    let night =
      parse(&FORECAST_JSON.replace(r#""is_day": 1, "uv_index""#, r#""is_day": 0, "uv_index""#));
    assert!(!night.current.is_day);
    let odd =
      parse(&FORECAST_JSON.replace(r#""is_day": 1, "uv_index""#, r#""is_day": 2, "uv_index""#));
    assert!(!odd.current.is_day);
    let hours = parse(&FORECAST_JSON.replace(r#""is_day": [1, 1]"#, r#""is_day": [0, 1]"#));
    assert_eq!(
      hours.hourly.iter().map(|h| h.is_day).collect::<Vec<_>>(),
      [false, true]
    );
  }

  #[test]
  fn out_of_range_codes_fail() {
    let json = FORECAST_JSON.replace(r#""weather_code": 3,"#, r#""weather_code": 300,"#);
    assert!(serde_json::from_str::<Forecast>(&json).is_err());
  }

  #[test]
  fn null_values_drop_only_their_hour_or_day() {
    let json = FORECAST_JSON
      .replace(
        r#""temperature_2m": [14.0, 14.4]"#,
        r#""temperature_2m": [null, 14.4]"#,
      )
      .replace(r#""sunrise": ["2026-10-01T07:12"]"#, r#""sunrise": [null]"#);
    let weather = parse(&json);
    assert_eq!(weather.hourly.len(), 1);
    assert_eq!(weather.hourly[0].temperature, 14.4);
    assert!(weather.daily.is_empty());
    assert_eq!(weather.current.temperature, 14.2);
  }

  #[test]
  fn geocode_finds_the_first_place() {
    let (client, urls) = fake(|_| {
      (
        200,
        r#"{"results": [{"name": "Bad Aibling", "latitude": 47.86, "longitude": 12.01, "country": "Germany"},
                        {"name": "Elsewhere", "latitude": 0, "longitude": 0}]}"#
          .into(),
      )
    });
    let location = block_on(geocode(&*client, " Bad Aibling , Bavaria")).unwrap();
    assert_eq!(
      location,
      Location {
        name: "Bad Aibling, Germany".into(),
        latitude: 47.86,
        longitude: 12.01,
        query: Some(" Bad Aibling , Bavaria".into()),
      }
    );
    let urls = urls.lock().unwrap();
    assert_eq!(
      urls.as_slice(),
      [format!(
        "{GEOCODING}?name=Bad%20Aibling&count=10&format=json"
      )]
    );
  }

  #[test]
  fn geocode_without_a_country() {
    let (client, _) = fake(|_| {
      (
        200,
        r#"{"results": [{"name": "Atlantis", "latitude": 1, "longitude": 2}]}"#.into(),
      )
    });
    assert_eq!(
      block_on(geocode(&*client, "Atlantis")).unwrap().name,
      "Atlantis"
    );
  }

  #[test]
  fn geocode_without_results() {
    for body in [r#"{}"#, r#"{"results": null}"#, r#"{"results": []}"#] {
      let (client, _) = fake(move |_| (200, body.into()));
      let error = block_on(geocode(&*client, "Nowhere")).unwrap_err();
      assert_eq!(error.to_string(), "no place named Nowhere");
    }
  }

  #[test]
  fn geocode_picks_the_region() {
    let (client, urls) = fake(|_| {
      (
        200,
        r#"{"results": [
          {"name": "Paris", "latitude": 48.85, "longitude": 2.35, "country": "France", "country_code": "FR", "admin1": "Ile-de-France"},
          {"name": "Paris", "latitude": 33.66, "longitude": -95.55, "country": "United States", "country_code": "US", "admin1": "Texas"}
        ]}"#
          .into(),
      )
    });
    let texas = block_on(geocode(&*client, "Paris, texas")).unwrap();
    assert_eq!(
      (texas.latitude, texas.name.as_str()),
      (33.66, "Paris, United States")
    );
    assert_eq!(texas.query.as_deref(), Some("Paris, texas"));
    assert!(urls.lock().unwrap()[0].contains("name=Paris&count=10"));
    // by country code too, and the first place when nothing matches
    assert_eq!(
      block_on(geocode(&*client, "Paris, FR")).unwrap().latitude,
      48.85
    );
    assert_eq!(
      block_on(geocode(&*client, "Paris, Atlantis"))
        .unwrap()
        .latitude,
      48.85
    );
    // a trailing comma is no region
    block_on(geocode(&*client, "Paris,")).unwrap();
    assert!(urls.lock().unwrap()[3].contains("count=1&"));
  }

  #[test]
  fn http_errors_carry_status_and_body() {
    let (client, _) = fake(|_| (503, "  try later \n".into()));
    let error = block_on(geocode(&*client, "Berlin"))
      .unwrap_err()
      .to_string();
    assert!(
      error.ends_with("answered 503 Service Unavailable: try later"),
      "{error}"
    );
    assert!(error.starts_with(GEOCODING), "{error}");
  }

  #[test]
  fn bad_json_names_the_url() {
    let (client, _) = fake(|_| (200, "<html>".into()));
    let error = block_on(super::forecast(&*client, here(), Units::Metric)).unwrap_err();
    assert!(error.to_string().starts_with(&format!(
      "parsing {FORECAST}?latitude=47.86&longitude=-12.5"
    )));
  }

  #[test]
  fn network_errors_pass_through() {
    let client = FakeHttpClient::create(|_| async { Err(anyhow::anyhow!("offline")) });
    let error = block_on(super::forecast(&*client, here(), Units::Metric)).unwrap_err();
    assert_eq!(error.to_string(), "offline");
  }

  #[test]
  fn forecast_queries_the_units() {
    let (client, urls) = fake(|_| (200, FORECAST_JSON.into()));
    let metric = block_on(super::forecast(&*client, here(), Units::Metric)).unwrap();
    let imperial = block_on(super::forecast(&*client, here(), Units::Imperial)).unwrap();
    assert_eq!(
      (metric.units, imperial.units),
      (Units::Metric, Units::Imperial)
    );
    let urls = urls.lock().unwrap();
    assert!(!urls[0].contains("fahrenheit"));
    assert!(urls[1].ends_with("&temperature_unit=fahrenheit&wind_speed_unit=mph"));
    for url in urls.iter() {
      assert!(url.starts_with(&format!(
        "{FORECAST}?latitude=47.86&longitude=-12.5&current="
      )));
      assert!(url.contains(&format!(
        "&forecast_days={DAYS}&forecast_hours={HOURS}&timezone=auto"
      )));
    }
  }

  #[test]
  fn geocode_multiple_commas_falls_back_to_first() {
    let (client, urls) = fake(|_| {
      (
        200,
        r#"{"results": [
          {"name": "Springfield", "latitude": 39.78, "longitude": -89.65, "country": "United States", "country_code": "US", "admin1": "Illinois"},
          {"name": "Springfield", "latitude": 37.21, "longitude": -93.29, "country": "United States", "country_code": "US", "admin1": "Missouri"}
        ]}"#
          .into(),
      )
    });
    // With multiple commas, region is "Illinois, US" which does not match admin1 "Illinois"
    let loc = block_on(geocode(&*client, "Springfield, Illinois, US")).unwrap();
    assert_eq!(loc.name, "Springfield, United States");
    assert_eq!(loc.latitude, 39.78);
    assert!(urls.lock().unwrap()[0].contains("name=Springfield&count=10"));
  }

  #[test]
  fn geocode_empty_before_comma() {
    let (client, urls) = fake(|_| {
      (
        200,
        r#"{"results": [
          {"name": "Texas City", "latitude": 29.38, "longitude": -94.90, "country": "United States"}
        ]}"#
          .into(),
      )
    });
    let loc = block_on(geocode(&*client, ", Texas")).unwrap();
    assert_eq!(loc.name, "Texas City, United States");
    assert!(urls.lock().unwrap()[0].contains("name=&count=10"));
  }

  #[test]
  fn geocode_non_ascii_region_case_folding() {
    // eq_ignore_ascii_case does not fold non-ASCII letters like è/È
    let (client, _) = fake(|_| {
      (
        200,
        r#"{"results": [
          {"name": "Genève", "latitude": 46.20, "longitude": 6.14, "country": "Suisse", "admin1": "Genève"},
          {"name": "Autre", "latitude": 10.0, "longitude": 20.0, "country": "Suisse", "admin1": "GENÈVE"}
        ]}"#
          .into(),
      )
    });
    // Exact match matches
    let exact = block_on(geocode(&*client, "Ville, Genève")).unwrap();
    assert_eq!(exact.latitude, 46.20);
    // Non-ASCII uppercase with different case does not match via eq_ignore_ascii_case, falls back to first place
    let upper = block_on(geocode(&*client, "Ville, genève")).unwrap();
    assert_eq!(upper.latitude, 46.20);
  }

  #[test]
  fn geocode_empty_country_string() {
    let (client, _) = fake(|_| {
      (
        200,
        r#"{"results": [
          {"name": "Vatican", "latitude": 41.90, "longitude": 12.45, "country": ""}
        ]}"#
          .into(),
      )
    });
    let loc = block_on(geocode(&*client, "Vatican")).unwrap();
    // Some("") formats as "Vatican, "
    assert_eq!(loc.name, "Vatican, ");
  }

  #[test]
  fn ragged_precipitation_probability_drops_hour() {
    // hourly precipitation_probability has 1 entry while time has 2 entries
    let json = FORECAST_JSON.replace(
      r#""precipitation_probability": [10, null]"#,
      r#""precipitation_probability": [10]"#,
    );
    let weather = parse(&json);
    // The second hour is dropped because precipitation_probability.get(1) is None
    assert_eq!(weather.hourly.len(), 1);
    assert_eq!(weather.hourly[0].precipitation_probability, 10.0);
  }

  #[test]
  fn null_or_ragged_sunset_drops_day() {
    // null sunset drops day
    let null_sunset = FORECAST_JSON.replace(
      r#""sunset": ["2026-10-01T18:54"]"#,
      r#""sunset": [null]"#,
    );
    let weather = parse(&null_sunset);
    assert!(weather.daily.is_empty());

    // ragged/empty sunset drops day
    let empty_sunset = FORECAST_JSON.replace(
      r#""sunset": ["2026-10-01T18:54"]"#,
      r#""sunset": []"#,
    );
    let weather2 = parse(&empty_sunset);
    assert!(weather2.daily.is_empty());
  }

  #[test]
  fn body_streaming_io_failure() {
    use std::io;
    use futures_lite::io::AsyncRead;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    struct FailingBody;
    impl AsyncRead for FailingBody {
      fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut [u8],
      ) -> Poll<io::Result<usize>> {
        Poll::Ready(Err(io::Error::new(io::ErrorKind::ConnectionReset, "body read stream reset")))
      }
    }

    let client = FakeHttpClient::create(|_| async {
      Ok(
        Response::builder()
          .status(200)
          .body(AsyncBody::from_reader(FailingBody))
          .unwrap(),
      )
    });
    let err = block_on(super::forecast(&*client, here(), Units::Metric)).unwrap_err();
    assert!(err.to_string().contains("body read stream reset"), "{err}");
  }
}
