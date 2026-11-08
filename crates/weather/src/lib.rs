use std::{path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use corona_config::{ConfigProvider, observe_section};
use corona_utils::{
  Buses,
  error::ErrorLogExt,
  persist::{load_json, save_json},
  ticker::wait_or_refresh,
};
use gpui_kit::{App, AppContext, Entity, Global, http_client::HttpClient};

pub use crate::state::{Condition, Current, Day, Hour, Location, Weather, compass};
pub use corona_config::Units;

mod api;
mod state;

const RETRY: Duration = Duration::from_secs(5 * 60);

#[derive(Clone)]
pub struct WeatherService {
  pub weather: Entity<Option<Weather>>,
  pub error: Entity<Option<String>>,
  /// Where the forecast is for, set by [`set_location`](WeatherService::set_location)
  place: Entity<Option<Location>>,
  /// Why there is no place
  place_error: Entity<Option<String>>,
  refresh: flume::Sender<()>,
}

impl Global for WeatherService {}

corona_utils::global_ext!(WeatherExt, weather, WeatherService);

impl WeatherService {
  pub fn current<'c>(&self, cx: &'c App) -> Option<&'c Weather> {
    self.weather.read(cx).as_ref()
  }

  pub fn error<'c>(&self, cx: &'c App) -> Option<&'c str> {
    self.error.read(cx).as_deref()
  }

  pub fn refresh(&self) {
    let _ = self.refresh.send(());
  }

  /// Moves the forecast to `place` and fetches it again
  pub fn set_location(&self, place: Location, cx: &mut App) {
    self.place.write(cx, Some(place));
    self.refresh();
  }

  /// Why the place is unknown, shown while there is none
  pub fn set_location_error(&self, error: String, cx: &mut App) {
    self.place_error.write(cx, Some(error));
  }
}

fn cache_path_in(cache_dir: Option<PathBuf>) -> Option<PathBuf> {
  Some(cache_dir?.join("corona").join("weather.json"))
}

fn cache_path() -> Option<PathBuf> {
  cache_path_in(dirs::cache_dir())
}

fn load_cache_from(path: Option<PathBuf>) -> Option<Weather> {
  load_json(&path?)
}

fn load_cache() -> Option<Weather> {
  load_cache_from(cache_path())
}

fn save_cache_to(path: Option<PathBuf>, weather: &Weather) -> Result<()> {
  path.map_or(Ok(()), |path| save_json(&path, weather))
}

fn save_cache(weather: &Weather) -> Result<()> {
  save_cache_to(cache_path(), weather)
}

/// Starts the forecast
pub fn init(cx: &mut App, _: &Buses) {
  start(cx);
}

fn start(cx: &mut App) {
  let (refresh, requests) = flume::unbounded();
  let state = WeatherService {
    weather: cx.new(|_| load_cache()),
    error: cx.new(|_| None),
    place: cx.new(|_| None),
    place_error: cx.new(|_| None),
    refresh,
  };

  let (weather, error) = (state.weather.clone(), state.error.clone());
  let (place, place_error) = (state.place.clone(), state.place_error.clone());
  let client: Arc<dyn HttpClient> = cx.http_client();
  cx.spawn(async move |cx| {
    loop {
      // this fetch reads the latest place and units, earlier requests are answered by it
      requests.drain();
      let (config, place) = cx.update(|cx| {
        let place = place.read(cx).clone().with_context(|| {
          place_error
            .read(cx)
            .as_deref()
            .unwrap_or("locating")
            .to_string()
        });
        (cx.config().weather.clone(), place)
      });
      let fetched = match place {
        Ok(place) => api::forecast(&*client, place, config.units).await,
        Err(e) => Err(e),
      };

      let wait = match fetched {
        Ok(next) => {
          save_cache(&next).log_err().ok();
          weather.write(cx, Some(next));
          error.write(cx, None);
          // a hand-edited huge value would overflow the timer instant
          Duration::from_secs(config.refresh_minutes.clamp(1, 24 * 60) * 60)
        }
        Err(e) => {
          tracing::warn!("weather: {e:?}");
          error.write(cx, Some(e.to_string()));
          RETRY
        }
      };
      if !wait_or_refresh(cx, &requests, wait).await {
        break;
      }
    }
  })
  .detach();

  cx.set_global(state);
  // fetch again right away for new units
  observe_section(
    cx,
    |c| &c.weather,
    |_, cx| cx.global::<WeatherService>().refresh(),
  );
}

#[cfg(test)]
mod tests {
  use std::{
    sync::{Arc, Mutex},
    time::SystemTime,
  };

  use corona_config::{Config, LocationConfig};
  use corona_utils::test_bus::TestBus;
  use futures_lite::future::pending;
  use gpui_kit::{self as gpui, TestAppContext};

  use super::*;
  use crate::api::tests::fake;

  const GEOCODE_JSON: &str = r#"{"results": [{"name": "Berlin", "latitude": 52.52, "longitude": 13.41, "country": "Germany"}]}"#;

  fn forecast_json() -> String {
    api::tests::FORECAST_JSON.to_string()
  }

  fn place(
    city: Option<&str>,
    coordinates: Option<(f64, f64)>,
    auto_locate: bool,
  ) -> LocationConfig {
    LocationConfig {
      city: city.map(Into::into),
      latitude: coordinates.map(|c| c.0),
      longitude: coordinates.map(|c| c.1),
      auto_locate,
    }
  }

  /// the cache lives under XDG_CACHE_HOME; nextest runs each test in its own process
  fn isolated_cache() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("XDG_CACHE_HOME", dir.path()) };
    dir
  }

  fn sample() -> Weather {
    Weather {
      location: Location {
        name: "x".into(),
        latitude: 1.,
        longitude: 2.,
        query: None,
      },
      units: Units::Imperial,
      elevation: 3.,
      timezone: "UTC".into(),
      timezone_abbreviation: "UTC".into(),
      current: Current {
        time: "t".into(),
        temperature: 1.,
        apparent_temperature: 2.,
        humidity: 3.,
        wind_speed: 4.,
        wind_direction: 5.,
        uv_index: 6.,
        code: 7,
        is_day: true,
      },
      hourly: vec![],
      daily: vec![],
      fetched: SystemTime::UNIX_EPOCH + Duration::from_secs(1234),
    }
  }

  #[test]
  fn cache_round_trip() {
    let dir = isolated_cache();
    assert!(load_cache().is_none());
    save_cache(&sample()).unwrap();
    assert_eq!(
      cache_path().unwrap(),
      dir.path().join("corona").join("weather.json")
    );
    assert_eq!(load_cache(), Some(sample()));
    std::fs::write(cache_path().unwrap(), b"{ broken").unwrap();
    assert!(load_cache().is_none());
  }

  #[test]
  fn cache_write_failures_are_errors() {
    let dir = isolated_cache();
    // a file where the directory should go
    std::fs::write(dir.path().join("corona"), b"").unwrap();
    assert!(save_cache(&sample()).is_err());
  }

  /// What the shell's wiring does
  fn follow_location(cx: &mut App) {
    use corona_location::{LocationEvent, LocationExt};
    let events = cx.location().events.clone();
    cx.subscribe(&events, |_, event, cx| {
      let weather = cx.weather().clone();
      match event {
        LocationEvent::Changed(place) => weather.set_location(place.clone(), cx),
        LocationEvent::Failed(e) => weather.set_location_error(e.clone(), cx),
      }
    })
    .detach();
    if let Some(place) = cx.location().current(cx).cloned() {
      cx.weather().clone().set_location(place, cx);
    }
  }

  struct Running {
    _bus: TestBus,
    _cache: tempfile::TempDir,
    urls: Arc<Mutex<Vec<String>>>,
    status: Arc<Mutex<u16>>,
  }

  fn start(cx: &mut TestAppContext, config: Config) -> Running {
    let cache = isolated_cache();
    let (bus, conn) = TestBus::with_conn();
    let status = Arc::new(Mutex::new(200));
    let answer = status.clone();
    let (client, urls) = fake(move |url| {
      let status = *answer.lock().unwrap();
      if status != 200 {
        (status, "down".into())
      } else if url.contains("geocoding") {
        (200, GEOCODE_JSON.into())
      } else {
        (200, forecast_json())
      }
    });
    cx.update(|cx| {
      cx.set_http_client(client);
      cx.set_global(config);
      corona_location::init(cx, &conn);
      super::start(cx);
      follow_location(cx);
    });
    cx.run_until_parked();
    Running {
      _bus: bus,
      _cache: cache,
      urls,
      status,
    }
  }

  fn berlin() -> Config {
    let mut config = Config {
      location: place(Some("Berlin"), None, false),
      ..Default::default()
    };
    config.weather.refresh_minutes = 10;
    config
  }

  fn requests(running: &Running) -> usize {
    running.urls.lock().unwrap().len()
  }

  #[gpui::test]
  fn fetches_and_caches(cx: &mut TestAppContext) {
    let running = start(cx, berlin());
    cx.read(|cx| {
      let service = cx.weather();
      let weather = service.current(cx).unwrap();
      assert_eq!(weather.location.name, "Berlin, Germany");
      assert_eq!(weather.current.temperature, 14.2);
      assert_eq!(service.error(cx), None);
    });
    // geocoding, then the forecast
    assert_eq!(requests(&running), 2);
    assert_eq!(load_cache().unwrap().location.name, "Berlin, Germany");
  }

  #[gpui::test]
  fn refreshes_on_schedule_reusing_the_place(cx: &mut TestAppContext) {
    let running = start(cx, berlin());
    cx.executor()
      .advance_clock(Duration::from_secs(10 * 60 - 1));
    assert_eq!(requests(&running), 2);
    cx.executor().advance_clock(Duration::from_secs(1));
    // only the forecast: the place is known
    assert_eq!(requests(&running), 3);
    assert!(running.urls.lock().unwrap()[2].contains("forecast"));
  }

  #[gpui::test]
  fn zero_refresh_minutes_means_one(cx: &mut TestAppContext) {
    let mut config = berlin();
    config.weather.refresh_minutes = 0;
    let running = start(cx, config);
    cx.executor().advance_clock(Duration::from_secs(60));
    assert_eq!(requests(&running), 3);
  }

  #[gpui::test]
  fn errors_retry_and_clear(cx: &mut TestAppContext) {
    let cache = isolated_cache();
    save_cache(&sample()).unwrap();
    let running = start(cx, berlin());
    drop(cache);
    // the first fetch worked; now the service goes down
    *running.status.lock().unwrap() = 500;
    cx.update(|cx| cx.weather().refresh());
    cx.run_until_parked();
    cx.read(|cx| {
      let service = cx.weather();
      let error = service.error(cx).unwrap();
      assert!(error.contains("500"));
      // the exact coordinates stay out of the shown error
      assert!(!error.contains("latitude") && !error.contains("52.5"));
      // the last weather stays
      assert_eq!(
        service.current(cx).unwrap().location.name,
        "Berlin, Germany"
      );
    });
    let failed = requests(&running);
    *running.status.lock().unwrap() = 200;
    cx.executor().advance_clock(RETRY - Duration::from_secs(1));
    assert_eq!(requests(&running), failed);
    cx.executor().advance_clock(Duration::from_secs(1));
    assert!(requests(&running) > failed);
    cx.read(|cx| assert_eq!(cx.weather().error(cx), None));
  }

  #[gpui::test]
  fn shows_the_cache_first(cx: &mut TestAppContext) {
    let cache = isolated_cache();
    save_cache(&sample()).unwrap();
    let (_bus, conn) = TestBus::with_conn();
    // never answers: only the cache can be showing
    let client = gpui_kit::http_client::FakeHttpClient::create(|_| pending());
    cx.update(|cx| {
      cx.set_http_client(client);
      cx.set_global(berlin());
      corona_location::init(cx, &conn);
      super::start(cx);
      follow_location(cx);
    });
    cx.run_until_parked();
    cx.read(|cx| assert_eq!(cx.weather().current(cx), Some(&sample())));
    drop(cache);
  }

  #[gpui::test]
  fn config_changes_fetch_right_away(cx: &mut TestAppContext) {
    let running = start(cx, berlin());
    cx.update(|cx| {
      let mut config = cx.config().clone();
      config.weather.units = Units::Imperial;
      cx.set_global(config);
    });
    cx.run_until_parked();
    assert_eq!(requests(&running), 3);
    assert!(running.urls.lock().unwrap()[2].contains("fahrenheit"));
    cx.read(|cx| assert_eq!(cx.weather().current(cx).unwrap().units, Units::Imperial));

    cx.update(|cx| {
      let mut config = cx.config().clone();
      config.location.city = Some("Munich".into());
      cx.set_global(config);
    });
    cx.run_until_parked();
    // a new city geocodes again
    assert_eq!(requests(&running), 5);

    // unrelated changes do not
    cx.update(|cx| {
      let mut config = cx.config().clone();
      config.osd.hide_delay_ms += 1;
      cx.set_global(config);
    });
    cx.run_until_parked();
    assert_eq!(requests(&running), 5);
  }

  #[gpui::test]
  fn manual_refresh(cx: &mut TestAppContext) {
    let running = start(cx, berlin());
    cx.update(|cx| cx.weather().refresh());
    cx.run_until_parked();
    assert_eq!(requests(&running), 3);
  }

  #[test]
  fn missing_cache_dir_handled_gracefully() {
    assert!(cache_path_in(None).is_none());
    assert!(load_cache_from(None).is_none());
    assert!(save_cache_to(None, &sample()).is_ok());
  }

  #[gpui::test]
  fn huge_refresh_minutes_boundary(cx: &mut TestAppContext) {
    let mut config = berlin();
    config.weather.refresh_minutes = 24 * 60; // 1 full day
    let running = start(cx, config);
    assert_eq!(requests(&running), 2);
  }

  #[gpui::test]
  fn refresh_burst_buffering(cx: &mut TestAppContext) {
    let running = start(cx, berlin());
    cx.update(|cx| {
      let service = cx.weather();
      service.refresh();
      service.refresh();
    });
    cx.run_until_parked();
    // a burst is answered by one fetch
    assert_eq!(requests(&running), 3);
  }

  #[gpui::test]
  fn background_task_terminates_on_disconnect(cx: &mut TestAppContext) {
    let running = start(cx, berlin());
    let before = requests(&running);
    cx.update(|cx| {
      let (dummy_tx, _) = flume::unbounded();
      let dummy_service = WeatherService {
        weather: cx.new(|_| None),
        error: cx.new(|_| None),
        place: cx.new(|_| None),
        place_error: cx.new(|_| None),
        refresh: dummy_tx,
      };
      cx.set_global(dummy_service);
    });
    cx.executor().advance_clock(Duration::from_secs(31 * 60));
    // the loop is gone, nothing fetches after the refresh interval
    assert_eq!(requests(&running), before);
  }

  #[gpui::test]
  fn huge_refresh_minutes_does_not_overflow(cx: &mut TestAppContext) {
    let mut config = berlin();
    config.weather.refresh_minutes = u64::MAX;
    let running = start(cx, config);
    assert_eq!(requests(&running), 2);
  }
}
