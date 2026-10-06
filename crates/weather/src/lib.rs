use std::{fs, path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Result, bail};
use corona_config::{ConfigProvider, LocationConfig, observe_section};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{App, AppContext, Entity, Global, http_client::HttpClient};
use zbus::Connection;

pub use crate::state::{Condition, Current, Day, Hour, Location, Weather, compass, weekday};
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
