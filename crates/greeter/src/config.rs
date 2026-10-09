use std::{
  fs,
  io::ErrorKind,
  path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use corona_config::ThemeConfig;
use serde::Deserialize;

const DIR: &str = "corona-greeter";

/// `~/.config/corona-greeter/config.toml` of the user the greeter runs as
pub fn config_file() -> Option<PathBuf> {
  Some(dirs::config_dir()?.join(DIR).join("config.toml"))
}

/// `~/.local/state/corona-greeter/<name>`, like `last-session` or `last-user`
pub fn state_file(name: &str) -> Option<PathBuf> {
  Some(dirs::state_dir()?.join(DIR).join(name))
}

#[derive(Deserialize, Debug, Default, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
  /// Like the shell's `[theme]`
  pub theme: ThemeConfig,
  /// Like `de`, over `LANG`
  pub language: Option<String>,
  /// The monitor showing the login, like `DP-1`. Unset or disconnected: the
  /// leftmost
  pub monitor: Option<String>,
}

/// The defaults without a file
pub fn read(path: &Path) -> Result<Config> {
  match fs::read_to_string(path) {
    Ok(text) => toml::from_str(&text).with_context(|| format!("bad {}", path.display())),
    Err(e) if e.kind() == ErrorKind::NotFound => Ok(Config::default()),
    Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
  }
}

pub fn read_state(path: &Path) -> Option<String> {
  let value = fs::read_to_string(path).ok()?;
  Some(value.trim().to_string()).filter(|value| !value.is_empty())
}

pub fn save_state(path: &Path, value: &str) -> Result<()> {
  if let Some(dir) = path.parent() {
    fs::create_dir_all(dir)?;
  }
  fs::write(path, value).with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn reads_theme_and_language() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    assert_eq!(read(&path).unwrap(), Config::default());

    fs::write(
      &path,
      "language = \"de\"\nmonitor = \"DP-1\"\n[theme]\nname = \"Catppuccin Mocha\"\n",
    )
    .unwrap();
    let config = read(&path).unwrap();
    assert_eq!(config.language.as_deref(), Some("de"));
    assert_eq!(config.monitor.as_deref(), Some("DP-1"));
    assert_eq!(config.theme.name, "Catppuccin Mocha");
    // the rest of the theme keeps its defaults
    assert_eq!(config.theme.font_scale, ThemeConfig::default().font_scale);

    fs::write(&path, "langauge = \"de\"").unwrap();
    assert!(read(&path).is_err());
  }

  #[test]
  fn remembers_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("corona-greeter/last-session");
    assert_eq!(read_state(&path), None);
    save_state(&path, "hyprland-uwsm").unwrap();
    assert_eq!(read_state(&path).as_deref(), Some("hyprland-uwsm"));
  }
}
