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

/// `~/.local/state/corona-greeter/last-session`: the id of the session started last
pub fn last_session_file() -> Option<PathBuf> {
  Some(dirs::state_dir()?.join(DIR).join("last-session"))
}

#[derive(Deserialize, Debug, Default, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
  /// Like the shell's `[theme]`
  pub theme: ThemeConfig,
  /// Like `de`, over `LANG`
  pub language: Option<String>,
}

/// The defaults without a file
pub fn read(path: &Path) -> Result<Config> {
  match fs::read_to_string(path) {
    Ok(text) => toml::from_str(&text).with_context(|| format!("bad {}", path.display())),
    Err(e) if e.kind() == ErrorKind::NotFound => Ok(Config::default()),
    Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
  }
}

pub fn last_session(path: &Path) -> Option<String> {
  let id = fs::read_to_string(path).ok()?;
  Some(id.trim().to_string()).filter(|id| !id.is_empty())
}

pub fn save_session(path: &Path, id: &str) -> Result<()> {
  if let Some(dir) = path.parent() {
    fs::create_dir_all(dir)?;
  }
  fs::write(path, id).with_context(|| format!("writing {}", path.display()))
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
      "language = \"de\"\n[theme]\nname = \"Catppuccin Mocha\"\n",
    )
    .unwrap();
    let config = read(&path).unwrap();
    assert_eq!(config.language.as_deref(), Some("de"));
    assert_eq!(config.theme.name, "Catppuccin Mocha");
    // the rest of the theme keeps its defaults
    assert_eq!(config.theme.font_scale, ThemeConfig::default().font_scale);

    fs::write(&path, "langauge = \"de\"").unwrap();
    assert!(read(&path).is_err());
  }

  #[test]
  fn remembers_the_session() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("corona-greeter/last-session");
    assert_eq!(last_session(&path), None);
    save_session(&path, "hyprland-uwsm").unwrap();
    assert_eq!(last_session(&path).as_deref(), Some("hyprland-uwsm"));
  }
}
