use std::path::PathBuf;

use anyhow::{Context, Result};
use gpui_kit::{App, Global};
use serde::{Deserialize, Serialize};

use crate::bar::BarConfig;

pub mod bar;
pub mod placement;
pub mod widget;

pub const APP_NAME: &str = "corona";

pub fn load(cx: &mut App) -> Result<()> {
  let config_dir = dirs::config_dir()
    .context("Failed to get config directory")?
    .join("corona");

  let config_dir_name = config_dir
    .to_str()
    .context("Failed to convert config path to string")?;

  let files = glob::glob(&format!("{}/**/*.toml", config_dir_name))
    .context("Failed to read config files")?
    .flatten()
    .map(config::File::from)
    .collect::<Vec<_>>();

  let config = config::Config::builder()
    .add_source(config::Config::try_from(&Config::default())?)
    .add_source(files)
    .add_source(config::Environment::with_prefix("CORONA"))
    .build()?
    .try_deserialize::<Config>()?;

  cx.set_global(config);

  Ok(())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
  pub theme: String,
  pub animation_speed: f32,
  pub bars: Vec<BarConfig>,
  pub plugin_dir: PathBuf,
  pub brightness: BrightnessConfig,
  pub weather: WeatherConfig,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Units {
  #[default]
  Metric,
  Imperial,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct WeatherConfig {
  pub city: Option<String>,
  pub latitude: Option<f64>,
  pub longitude: Option<f64>,
  pub auto_locate: bool,
  pub units: Units,
  pub refresh_minutes: u64,
}

impl Default for WeatherConfig {
  fn default() -> Self {
    Self {
      city: None,
      latitude: None,
      longitude: None,
      auto_locate: false,
      units: Units::Metric,
      refresh_minutes: 30,
    }
  }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BrightnessConfig {
  pub enable_ddcutil: bool,
}

impl Default for BrightnessConfig {
  fn default() -> Self {
    Self {
      enable_ddcutil: true,
    }
  }
}

pub trait ConfigProvider {
  fn config(&self) -> &Config;
}

impl ConfigProvider for App {
  fn config(&self) -> &Config {
    self.global::<Config>()
  }
}

impl Global for Config {}

impl Default for Config {
  fn default() -> Self {
    Self {
      theme: "shadcn Zinc Blue Dark".to_string(),
      animation_speed: 1.,
      bars: vec![Default::default()],
      plugin_dir: dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("corona/plugins"),
      brightness: BrightnessConfig::default(),
      weather: WeatherConfig::default(),
    }
  }
}
