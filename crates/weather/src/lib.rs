use std::{fs, path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Result, bail};
use corona_config::{ConfigProvider, LocationConfig, observe_section};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{App, AppContext, Entity, Global, http_client::HttpClient};
use zbus::Connection;

pub use crate::state::{Condition, Current, Day, Hour, Location, Weather, compass};
pub use corona_config::Units;

mod api;
mod locate;
mod state;

const LOCATE_TIMEOUT: Duration = Duration::from_secs(10);
const RETRY: Duration = Duration::from_secs(5 * 60);

#[derive(Clone)]
pub struct WeatherService {
  pub weather: Entity<Option<Weather>>,
  pub error: Entity<Option<String>>,
  refresh: flume::Sender<()>,
}

impl Global for WeatherService {}

pub trait WeatherExt {
  fn weather(&self) -> &WeatherService;
}

impl WeatherExt for App {
  fn weather(&self) -> &WeatherService {
    self.global::<WeatherService>()
  }
}

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
}

fn cache_path() -> Option<PathBuf> {
  Some(dirs::cache_dir()?.join("corona").join("weather.json"))
}

fn load_cache() -> Option<Weather> {
  serde_json::from_slice(&fs::read(cache_path()?).ok()?).ok()
}

fn save_cache(weather: &Weather) -> Result<()> {
  let Some(path) = cache_path() else {
    return Ok(());
  };
  if let Some(dir) = path.parent() {
    fs::create_dir_all(dir)?;
  }
  Ok(fs::write(path, serde_json::to_vec(weather)?)?)
}

async fn location(
  client: &dyn HttpClient,
  system: &Connection,
  config: &LocationConfig,
  previous: Option<&Location>,
  timeout: impl Future<Output = ()>,
) -> Result<Location> {
  if let (Some(latitude), Some(longitude)) = (config.latitude, config.longitude) {
    return Ok(Location {
      name: config
        .city
        .clone()
        .unwrap_or_else(|| format!("{latitude:.2}, {longitude:.2}")),
      latitude,
      longitude,
      query: None,
    });
  }
  if config.auto_locate {
    match locate::locate(system, timeout).await {
      Ok(location) => return Ok(location),
      Err(e) => tracing::warn!("GeoClue: {e:?}, falling back to the configured city"),
    }
  }
  let Some(city) = &config.city else {
    bail!("set city or latitude and longitude under [location]");
  };
  if let Some(previous) = previous.filter(|l| l.query.as_ref() == Some(city)) {
    return Ok(previous.clone());
  }
  api::geocode(client, city).await
}

pub fn init(cx: &mut App, system: &Connection) {
  let (refresh, requests) = flume::unbounded();
  let state = WeatherService {
    weather: cx.new(|_| load_cache()),
    error: cx.new(|_| None),
    refresh,
  };

  let (weather, error) = (state.weather.clone(), state.error.clone());
  let system = system.clone();
  let client: Arc<dyn HttpClient> = cx.http_client();
  cx.spawn(async move |cx| {
    loop {
      let (config, place) =
        cx.update(|cx| (cx.config().weather.clone(), cx.config().location.clone()));
      let previous = cx.update(|cx| weather.read(cx).as_ref().map(|w| w.location.clone()));
      let timer = cx.background_executor().timer(LOCATE_TIMEOUT);
      let fetched = async {
        let location = location(&*client, &system, &place, previous.as_ref(), timer).await?;
        api::forecast(&*client, location, config.units).await
      }
      .await;

      let wait = match fetched {
        Ok(next) => {
          save_cache(&next).log_err().ok();
          weather.write(cx, Some(next));
          error.write(cx, None);
          Duration::from_secs(config.refresh_minutes.max(1) * 60)
        }
        Err(e) => {
          tracing::warn!("weather: {e:?}");
          error.write(cx, Some(e.to_string()));
          RETRY
        }
      };
      let next = futures_lite::future::or(
        async {
          cx.background_executor().timer(wait).await;
          true
        },
        async { requests.recv_async().await.is_ok() },
      )
      .await;
      if !next {
        break;
      }
    }
  })
  .detach();

  cx.set_global(state);
  // fetch again right away for the new place or units
  observe_section(
    cx,
    |c| &c.location,
    |_, cx| cx.global::<WeatherService>().refresh(),
  );
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

  use corona_config::Config;
  use corona_utils::test_bus::TestBus;
  use futures_lite::future::{block_on, pending, ready};
  use gpui_kit::{self as gpui, TestAppContext};
  use zbus::{interface, object_server::SignalEmitter, zvariant::ObjectPath};

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

  struct Manager;

  #[interface(name = "org.freedesktop.GeoClue2.Manager")]
  impl Manager {
    fn get_client(&self) -> ObjectPath<'static> {
      ObjectPath::from_static_str_unchecked("/org/freedesktop/GeoClue2/Client/1")
    }
  }

  #[derive(Default)]
  struct Client {
    desktop_id: String,
    accuracy: u32,
    /// none: never answers
    fix: Option<&'static str>,
  }

  #[interface(name = "org.freedesktop.GeoClue2.Client")]
  impl Client {
    async fn start(&self, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>) {
      if let Some(fix) = self.fix {
        Self::location_updated(
          &emitter,
          ObjectPath::from_static_str_unchecked("/"),
          ObjectPath::from_static_str_unchecked(fix),
        )
        .await
        .unwrap();
      }
    }

    fn stop(&self) {}

    #[zbus(signal)]
    async fn location_updated(
      emitter: &SignalEmitter<'_>,
      old: ObjectPath<'_>,
      new: ObjectPath<'_>,
    ) -> zbus::Result<()>;

    #[zbus(property)]
    fn desktop_id(&self) -> String {
      self.desktop_id.clone()
    }

    #[zbus(property)]
    fn set_desktop_id(&mut self, value: String) {
      self.desktop_id = value;
    }

    #[zbus(property)]
    fn requested_accuracy_level(&self) -> u32 {
      self.accuracy
    }

    #[zbus(property)]
    fn set_requested_accuracy_level(&mut self, value: u32) {
      self.accuracy = value;
    }
  }

  struct Fix;

  #[interface(name = "org.freedesktop.GeoClue2.Location")]
  impl Fix {
    #[zbus(property)]
    fn latitude(&self) -> f64 {
      48.14
    }

    #[zbus(property)]
    fn longitude(&self) -> f64 {
      11.58
    }
  }

  /// a GeoClue on `bus` whose client answers with `fix`; keep the connection alive
  async fn geoclue(bus: &TestBus, fix: Option<&'static str>) -> zbus::Connection {
    let service = bus.conn().await;
    let server = service.object_server();
    server
      .at("/org/freedesktop/GeoClue2/Manager", Manager)
      .await
      .unwrap();
    server
      .at(
        "/org/freedesktop/GeoClue2/Client/1",
        Client {
          fix,
          ..Default::default()
        },
      )
      .await
      .unwrap();
    server
      .at("/org/freedesktop/GeoClue2/Location/1", Fix)
      .await
      .unwrap();
    service
      .request_name("org.freedesktop.GeoClue2")
      .await
      .unwrap();
    service
  }

  #[test]
  fn coordinates_win() {
    let bus = TestBus::new();
    block_on(async {
      let conn = bus.conn().await;
      let (client, urls) = fake(|_| unreachable!());
      let named = location(
        &*client,
        &conn,
        &place(Some("Home"), Some((1.234, -5.678)), true),
        None,
        ready(()),
      )
      .await
      .unwrap();
      assert_eq!(
        named,
        Location {
          name: "Home".into(),
          latitude: 1.234,
          longitude: -5.678,
          query: None
        }
      );
      let unnamed = location(
        &*client,
        &conn,
        &place(None, Some((1.234, -5.678)), false),
        None,
        ready(()),
      )
      .await
      .unwrap();
      assert_eq!(unnamed.name, "1.23, -5.68");
      assert!(urls.lock().unwrap().is_empty());
    });
  }

  #[test]
  fn half_coordinates_fall_back_to_the_city() {
    let bus = TestBus::new();
    block_on(async {
      let conn = bus.conn().await;
      let (client, _) = fake(|_| (200, GEOCODE_JSON.into()));
      let mut config = place(Some("Berlin"), None, false);
      config.latitude = Some(1.);
      let found = location(&*client, &conn, &config, None, ready(()))
        .await
        .unwrap();
      assert_eq!(found.name, "Berlin, Germany");
    });
  }

  #[test]
  fn nothing_configured() {
    let bus = TestBus::new();
    block_on(async {
      let conn = bus.conn().await;
      let (client, _) = fake(|_| unreachable!());
      let error = location(&*client, &conn, &place(None, None, false), None, ready(()))
        .await
        .unwrap_err();
      assert_eq!(
        error.to_string(),
        "set city or latitude and longitude under [location]"
      );
    });
  }

  #[test]
  fn the_previous_place_is_reused() {
    let bus = TestBus::new();
    block_on(async {
      let conn = bus.conn().await;
      let (client, urls) = fake(|_| (200, GEOCODE_JSON.into()));
      let previous = Location {
        name: "Berlin, Germany".into(),
        latitude: 52.52,
        longitude: 13.41,
        query: Some("Berlin".into()),
      };
      let config = place(Some("Berlin"), None, false);
      assert_eq!(
        location(&*client, &conn, &config, Some(&previous), ready(()))
          .await
          .unwrap(),
        previous
      );
      assert!(urls.lock().unwrap().is_empty());
      // another city asks again
      let config = place(Some("Munich"), None, false);
      location(&*client, &conn, &config, Some(&previous), ready(()))
        .await
        .unwrap();
      assert_eq!(urls.lock().unwrap().len(), 1);
    });
  }

  #[test]
  fn geoclue_locates() {
    let bus = TestBus::new();
    block_on(async {
      let service = geoclue(&bus, Some("/org/freedesktop/GeoClue2/Location/1")).await;
      let conn = bus.conn().await;
      let (client, urls) = fake(|_| unreachable!());
      let found = location(
        &*client,
        &conn,
        &place(Some("Berlin"), None, true),
        None,
        pending(),
      )
      .await
      .unwrap();
      assert_eq!(
        found,
        Location {
          name: String::new(),
          latitude: 48.14,
          longitude: 11.58,
          query: None
        }
      );
      assert!(urls.lock().unwrap().is_empty());

      let client_object = service
        .object_server()
        .interface::<_, Client>("/org/freedesktop/GeoClue2/Client/1")
        .await
        .unwrap();
      assert_eq!(client_object.get().await.desktop_id, "corona");
    });
  }

  #[test]
  fn geoclue_is_asked_for_city_accuracy() {
    let bus = TestBus::new();
    block_on(async {
      let service = geoclue(&bus, Some("/org/freedesktop/GeoClue2/Location/1")).await;
      let conn = bus.conn().await;
      let (client, _) = fake(|_| unreachable!());
      location(&*client, &conn, &place(None, None, true), None, pending())
        .await
        .unwrap();
      let client_object = service
        .object_server()
        .interface::<_, Client>("/org/freedesktop/GeoClue2/Client/1")
        .await
        .unwrap();
      assert_eq!(client_object.get().await.accuracy, 4);
    });
  }

  #[test]
  fn geoclue_timeout_falls_back_to_the_city() {
    let bus = TestBus::new();
    block_on(async {
      let _service = geoclue(&bus, None).await;
      let conn = bus.conn().await;
      let (client, _) = fake(|_| (200, GEOCODE_JSON.into()));
      let found = location(
        &*client,
        &conn,
        &place(Some("Berlin"), None, true),
        None,
        ready(()),
      )
      .await
      .unwrap();
      assert_eq!(found.query.as_deref(), Some("Berlin"));
    });
  }

  #[test]
  fn missing_geoclue_falls_back_to_the_city() {
    let bus = TestBus::new();
    block_on(async {
      let conn = bus.conn().await;
      let (client, _) = fake(|_| (200, GEOCODE_JSON.into()));
      let found = location(
        &*client,
        &conn,
        &place(Some("Berlin"), None, true),
        None,
        pending(),
      )
      .await
      .unwrap();
      assert_eq!(found.name, "Berlin, Germany");
      // and without a city the error says what to configure
      let error = location(&*client, &conn, &place(None, None, true), None, pending()).await;
      assert!(error.is_err());
    });
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
    fs::write(cache_path().unwrap(), b"{ broken").unwrap();
    assert!(load_cache().is_none());
  }

  #[test]
  fn cache_write_failures_are_errors() {
    let dir = isolated_cache();
    // a file where the directory should go
    fs::write(dir.path().join("corona"), b"").unwrap();
    assert!(save_cache(&sample()).is_err());
  }

  struct Running {
    _bus: TestBus,
    _cache: tempfile::TempDir,
    urls: Arc<Mutex<Vec<String>>>,
    status: Arc<Mutex<u16>>,
  }

  fn start(cx: &mut TestAppContext, config: Config) -> Running {
    let cache = isolated_cache();
    let bus = TestBus::new();
    let conn = block_on(bus.conn());
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
      init(cx, &conn);
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
      assert!(service.error(cx).unwrap().contains("500"));
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
    let bus = TestBus::new();
    let conn = block_on(bus.conn());
    // never answers: only the cache can be showing
    let client = gpui_kit::http_client::FakeHttpClient::create(|_| pending());
    cx.update(|cx| {
      cx.set_http_client(client);
      cx.set_global(berlin());
      init(cx, &conn);
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
}
