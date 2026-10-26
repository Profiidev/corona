//! Where the device is: configured coordinates, GeoClue or a geocoded city, kept current for
//! weather and nightlight.

use std::{fs, path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Result, bail};
use corona_config::{ConfigProvider, LocationConfig, observe_section};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{App, AppContext, Entity, Global, http_client::HttpClient};
use serde::{Deserialize, Serialize};
use zbus::Connection;

mod geocode;
mod locate;

const LOCATE_TIMEOUT: Duration = Duration::from_secs(10);
const RETRY: Duration = Duration::from_secs(5 * 60);
/// GeoClue follows a moving device this often
const REFRESH: Duration = Duration::from_secs(30 * 60);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Location {
  /// Empty for the device's own position, which the shell names
  pub name: String,
  pub latitude: f64,
  pub longitude: f64,
  pub query: Option<String>,
}

#[derive(Clone)]
pub struct LocationService {
  /// The last known place, from the cache until the first lookup
  pub location: Entity<Option<Location>>,
  pub error: Entity<Option<String>>,
  refresh: flume::Sender<()>,
}

impl Global for LocationService {}

pub trait LocationExt {
  fn location(&self) -> &LocationService;
}

impl LocationExt for App {
  fn location(&self) -> &LocationService {
    self.global::<LocationService>()
  }
}

impl LocationService {
  pub fn current<'c>(&self, cx: &'c App) -> Option<&'c Location> {
    self.location.read(cx).as_ref()
  }

  pub fn error<'c>(&self, cx: &'c App) -> Option<&'c str> {
    self.error.read(cx).as_deref()
  }

  pub fn refresh(&self) {
    let _ = self.refresh.send(());
  }
}

fn cache_path_in(cache_dir: Option<PathBuf>) -> Option<PathBuf> {
  Some(cache_dir?.join("corona").join("location.json"))
}

fn cache_path() -> Option<PathBuf> {
  cache_path_in(dirs::cache_dir())
}

fn load_cache() -> Option<Location> {
  serde_json::from_slice(&fs::read(cache_path()?).ok()?).ok()
}

fn save_cache(location: &Location) -> Result<()> {
  let Some(path) = cache_path() else {
    return Ok(());
  };
  if let Some(dir) = path.parent() {
    fs::create_dir_all(dir)?;
  }
  Ok(fs::write(path, serde_json::to_vec(location)?)?)
}

async fn resolve(
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
  geocode::geocode(client, city).await
}

pub fn init(cx: &mut App, system: &Connection) {
  let (refresh, requests) = flume::unbounded();
  let state = LocationService {
    location: cx.new(|_| load_cache()),
    error: cx.new(|_| None),
    refresh,
  };

  let (location, error) = (state.location.clone(), state.error.clone());
  let system = system.clone();
  let client: Arc<dyn HttpClient> = cx.http_client();
  cx.spawn(async move |cx| {
    loop {
      let config = cx.update(|cx| cx.config().location.clone());
      let previous = cx.update(|cx| location.read(cx).clone());
      let timer = cx.background_executor().timer(LOCATE_TIMEOUT);
      let wait = match resolve(&*client, &system, &config, previous.as_ref(), timer).await {
        Ok(next) => {
          // observers refetch only when the place moved
          if previous.as_ref() != Some(&next) {
            save_cache(&next).log_err().ok();
            location.write(cx, Some(next));
          }
          error.write(cx, None);
          REFRESH
        }
        Err(e) => {
          tracing::warn!("location: {e:?}");
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
  observe_section(
    cx,
    |c| &c.location,
    |_, cx| cx.global::<LocationService>().refresh(),
  );
}

#[cfg(test)]
mod tests {
  use std::sync::{Arc, Mutex};

  use corona_config::Config;
  use corona_utils::test_bus::TestBus;
  use futures_lite::future::{block_on, pending, ready};
  use gpui_kit::{self as gpui, TestAppContext};
  use zbus::{interface, object_server::SignalEmitter, zvariant::ObjectPath};

  use super::*;
  use crate::geocode::tests::fake;

  const GEOCODE_JSON: &str = r#"{"results": [{"name": "Berlin", "latitude": 52.52, "longitude": 13.41, "country": "Germany"}]}"#;

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
      let named = resolve(
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
      let unnamed = resolve(
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
      let found = resolve(&*client, &conn, &config, None, ready(()))
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
      let error = resolve(&*client, &conn, &place(None, None, false), None, ready(()))
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
        resolve(&*client, &conn, &config, Some(&previous), ready(()))
          .await
          .unwrap(),
        previous
      );
      assert!(urls.lock().unwrap().is_empty());
      // another city asks again
      let config = place(Some("Munich"), None, false);
      resolve(&*client, &conn, &config, Some(&previous), ready(()))
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
      let found = resolve(
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
      resolve(&*client, &conn, &place(None, None, true), None, pending())
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
      let found = resolve(
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
      let found = resolve(
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
      let error = resolve(&*client, &conn, &place(None, None, true), None, pending()).await;
      assert!(error.is_err());
    });
  }
  #[test]
  fn non_finite_and_extreme_coordinates() {
    let bus = TestBus::new();
    block_on(async {
      let conn = bus.conn().await;
      let (client, _) = fake(|_| unreachable!());
      let nan_loc = resolve(
        &*client,
        &conn,
        &place(None, Some((f64::NAN, f64::INFINITY)), false),
        None,
        ready(()),
      )
      .await
      .unwrap();
      assert_eq!(nan_loc.name, "NaN, inf");
      assert!(nan_loc.latitude.is_nan());
      assert_eq!(nan_loc.longitude, f64::INFINITY);

      let extreme = resolve(
        &*client,
        &conn,
        &place(None, Some((999.0, -999.0)), false),
        None,
        ready(()),
      )
      .await
      .unwrap();
      assert_eq!(extreme.name, "999.00, -999.00");
    });
  }

  #[test]
  fn empty_string_city_with_coordinates() {
    let bus = TestBus::new();
    block_on(async {
      let conn = bus.conn().await;
      let (client, _) = fake(|_| unreachable!());
      let empty_city = resolve(
        &*client,
        &conn,
        &place(Some(""), Some((12.34, 56.78)), false),
        None,
        ready(()),
      )
      .await
      .unwrap();
      assert_eq!(empty_city.name, "");
      assert_eq!(empty_city.latitude, 12.34);
      assert_eq!(empty_city.longitude, 56.78);
    });
  }

  #[test]
  fn geoclue_stream_closed_errors() {
    let bus = TestBus::new();
    block_on(async {
      let conn = bus.conn().await;
      let service = geoclue(&bus, None).await;
      drop(service);
      let result = locate::locate(&conn, pending()).await;
      assert!(result.is_err());
    });
  }

  #[test]
  fn geoclue_property_read_failure() {
    let bus = TestBus::new();
    block_on(async {
      // Points to a path that does not exist on object server
      let _service = geoclue(&bus, Some("/org/freedesktop/GeoClue2/Location/nonexistent")).await;
      let conn = bus.conn().await;
      let result = locate::locate(&conn, pending()).await;
      assert!(result.is_err());
    });
  }
  #[test]
  fn cache_round_trip() {
    let dir = isolated_cache();
    assert!(load_cache().is_none());
    let place = Location {
      name: "x".into(),
      latitude: 1.,
      longitude: 2.,
      query: Some("x".into()),
    };
    save_cache(&place).unwrap();
    assert_eq!(
      cache_path().unwrap(),
      dir.path().join("corona").join("location.json")
    );
    assert_eq!(load_cache(), Some(place));
    fs::write(cache_path().unwrap(), b"{ broken").unwrap();
    assert!(load_cache().is_none());
    assert!(cache_path_in(None).is_none());
  }

  struct Running {
    _bus: TestBus,
    _cache: tempfile::TempDir,
    urls: Arc<Mutex<Vec<String>>>,
    notified: Arc<Mutex<usize>>,
  }

  fn start(cx: &mut TestAppContext, config: Config) -> Running {
    let cache = isolated_cache();
    let bus = TestBus::new();
    let conn = block_on(bus.conn());
    let (client, urls) = fake(|url| {
      if url.contains("Nowhere") {
        (200, "{}".into())
      } else {
        (200, GEOCODE_JSON.into())
      }
    });
    let notified = Arc::new(Mutex::new(0));
    cx.update(|cx| {
      cx.set_http_client(client);
      cx.set_global(config);
      init(cx, &conn);
      let count = notified.clone();
      cx.observe(&cx.location().location.clone(), move |_, _| {
        *count.lock().unwrap() += 1
      })
      .detach();
    });
    cx.run_until_parked();
    Running {
      _bus: bus,
      _cache: cache,
      urls,
      notified,
    }
  }

  fn city(name: &str) -> Config {
    Config {
      location: place(Some(name), None, false),
      ..Default::default()
    }
  }

  #[gpui::test]
  fn resolves_caches_and_follows_the_config(cx: &mut TestAppContext) {
    let running = start(cx, city("Berlin"));
    cx.read(|cx| {
      assert_eq!(cx.location().current(cx).unwrap().name, "Berlin, Germany");
      assert_eq!(cx.location().error(cx), None);
    });
    assert_eq!(load_cache().unwrap().query.as_deref(), Some("Berlin"));
    assert_eq!(*running.notified.lock().unwrap(), 1);

    // the same place again does not notify
    cx.executor().advance_clock(REFRESH);
    assert_eq!(running.urls.lock().unwrap().len(), 1);
    assert_eq!(*running.notified.lock().unwrap(), 1);

    cx.update(|cx| cx.set_global(city("Nowhere")));
    cx.run_until_parked();
    cx.read(|cx| {
      assert_eq!(cx.location().error(cx), Some("no place named Nowhere"));
      // the last place stays
      assert_eq!(cx.location().current(cx).unwrap().name, "Berlin, Germany");
    });
  }

  #[gpui::test]
  fn shows_the_cache_first(cx: &mut TestAppContext) {
    let cache = isolated_cache();
    let cached = Location {
      name: "Cached".into(),
      latitude: 1.,
      longitude: 2.,
      query: None,
    };
    save_cache(&cached).unwrap();
    let bus = TestBus::new();
    let conn = block_on(bus.conn());
    let client = gpui_kit::http_client::FakeHttpClient::create(|_| pending());
    cx.update(|cx| {
      cx.set_http_client(client);
      cx.set_global(city("Berlin"));
      init(cx, &conn);
    });
    cx.run_until_parked();
    cx.read(|cx| assert_eq!(cx.location().current(cx), Some(&cached)));
    drop(cache);
  }
}
