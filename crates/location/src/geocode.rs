//! Open-Meteo geocoding: free, no API key.

use anyhow::{Context, Result, bail};
use futures_lite::AsyncReadExt;
use gpui_kit::http_client::{AsyncBody, HttpClient};
use serde::{Deserialize, de::DeserializeOwned};

use crate::Location;

const GEOCODING: &str = "https://geocoding-api.open-meteo.com/v1/search";

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

#[cfg(test)]
pub(crate) mod tests {
  use super::*;

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
}
