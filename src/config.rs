use std::time::Duration;

use anyhow::{Context, Result};
use gpui_kit::{App, Global};
use serde::{Deserialize, Serialize};

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
    .build()?
    .try_deserialize::<Config>()?;

  cx.set_global(config);

  Ok(())
}

#[derive(Serialize, Deserialize)]
pub struct Config {
  pub theme: String,
  pub animation_speed: AnimationSpeed,
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
      theme: "macOS Classic Dark".to_string(),
      animation_speed: Default::default(),
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default)]
pub enum AnimationSpeed {
  Slow,
  #[default]
  Normal,
  Fast,
}

impl AnimationSpeed {
  pub fn to_duration(self) -> Duration {
    match self {
      AnimationSpeed::Slow => Duration::from_millis(500),
      AnimationSpeed::Normal => Duration::from_millis(250),
      AnimationSpeed::Fast => Duration::from_millis(100),
    }
  }
}
