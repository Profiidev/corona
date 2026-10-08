use std::{
  collections::BTreeMap,
  path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow};
use gpui_kit::App;

use crate::{Config, ConfigProvider, IdleConfig, bar::BarConfig};

/// `~/.config/corona`, the user's own files
pub fn config_dir() -> Result<PathBuf> {
  Ok(
    dirs::config_dir()
      .context("Failed to get config directory")?
      .join("corona"),
  )
}

/// `~/.local/state/corona/settings.toml`, what the shell itself changed, over the
/// user's files
pub fn settings_file() -> Result<PathBuf> {
  Ok(
    dirs::state_dir()
      .context("Failed to get state directory")?
      .join("corona/settings.toml"),
  )
}

/// Every `*.toml` below `dir`, in the order they apply: later ones win
pub fn config_files(dir: &Path) -> Result<Vec<PathBuf>> {
  let pattern = dir.join("**/*.toml");
  let pattern = pattern.to_str().context("Config path is not UTF-8")?;
  let mut files: Vec<_> = glob::glob(pattern)?.flatten().collect();
  files.sort();
  Ok(files)
}

#[derive(Debug)]
pub struct Loaded {
  pub config: Config,
  /// Keys no setting knows, like `notification.timout`
  pub unknown: Vec<String>,
}

impl Loaded {
  pub fn warn_unknown(&self) {
    for key in &self.unknown {
      tracing::warn!("unknown setting {key}");
    }
  }
}

/// The settings the shell runs with: the config files, the settings file over
/// them, environment variables over both (`CORONA_SHELL__PLUGIN_DIR` sets
/// `shell.plugin_dir`).
pub fn read() -> Result<Loaded> {
  let mut files = config_files(&config_dir()?)?;
  files.push(settings_file()?);
  read_files(&files)
}

/// `files` merged in order, missing ones skipped. An error names the file it is in.
pub fn read_files(files: &[PathBuf]) -> Result<Loaded> {
  match deserialize(files) {
    Ok(loaded) => Ok(loaded),
    // config-rs names the key of a bad value but not its file, so find the file
    // that is bad on its own
    Err(e) => match files
      .iter()
      .find_map(|f| deserialize(std::slice::from_ref(f)).err().map(|e| (f, e)))
    {
      // config-rs names the file itself for some errors
      Some((file, e))
        if e
          .to_string()
          .contains(&*file.file_name().unwrap_or_default().to_string_lossy()) =>
      {
        Err(e)
      }
      Some((file, e)) => Err(anyhow!("{}: {e}", file.display())),
      None => Err(e),
    },
  }
}

fn deserialize(files: &[PathBuf]) -> Result<Loaded> {
  let built = files
    .iter()
    .fold(defaults()?, |builder, file| {
      builder.add_source(config::File::from(file.as_path()).required(false))
    })
    .add_source(config::Environment::with_prefix("CORONA").separator("__"))
    .build()?;
  let mut unknown = Vec::new();
  let mut config: Config = serde_ignored::deserialize(built, |key| unknown.push(key.to_string()))?;
  if let Some(bars) = last_bars(files)? {
    config.bar = bars;
  }
  Ok(Loaded { config, unknown })
}

/// The bottom layer: defaults that sit in maps, which serde would otherwise
/// drop whole once a file sets any entry. Idle behaviors merge into them per
/// field, so turning one off keeps the others and its own action.
fn defaults() -> Result<config::ConfigBuilder<config::builder::DefaultState>> {
  let mut table = toml::Table::new();
  table.insert(
    "idle".to_string(),
    toml::Value::try_from(IdleConfig::default())?,
  );
  let source = config::File::from_str(&toml::to_string(&table)?, config::FileFormat::Toml);
  Ok(config::Config::builder().add_source(source))
}

/// The bars of the last file that has any. Bars are taken whole from one layer
/// rather than merged, so a later layer can drop a bar as well as add one.
fn last_bars(files: &[PathBuf]) -> Result<Option<BTreeMap<String, BarConfig>>> {
  for file in files.iter().rev() {
    let Ok(text) = std::fs::read_to_string(file) else {
      continue;
    };
    let mut table: toml::Table = toml::from_str(&text)?;
    if let Some(bars) = table.remove("bar") {
      return Ok(Some(bars.try_into()?));
    }
  }
  Ok(None)
}

/// Makes `loaded` the settings, unless nothing changed.
pub(crate) fn apply(loaded: Loaded, cx: &mut App) {
  loaded.warn_unknown();
  if *cx.config() != loaded.config {
    cx.set_global(loaded.config);
    cx.refresh_windows();
  }
}

#[cfg(test)]
mod tests {
  use std::fs;

  use super::read_files;
  use crate::{Config, ThemeMode};

  #[test]
  fn layers() {
    let dir = std::env::temp_dir().join(format!("corona-config-test-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let write = |name: &str, content: &str| {
      let path = dir.join(name);
      fs::write(&path, content).unwrap();
      path
    };
    let a = write(
      "a.toml",
      "[theme]\nname = \"A\"\nmode = \"dark\"\n[osd]\nhide_delay_ms = 900\n",
    );
    let b = write(
      "b.toml",
      "[theme]\nname = \"B\"\n[notification]\ntimout_ms = 1\n",
    );
    let missing = dir.join("settings.toml");

    let loaded = read_files(&[a.clone(), b.clone(), missing]).unwrap();
    // later files win, per key
    assert_eq!(loaded.config.theme.name, "B");
    assert_eq!(loaded.config.theme.mode, Some(ThemeMode::Dark));
    assert_eq!(loaded.config.osd.hide_delay_ms, 900);
    // everything else defaults, the bar too
    assert_eq!(loaded.config.taskbar, Config::default().taskbar);
    assert_eq!(loaded.config.bar, Config::default().bar);
    // the environment may add its own
    assert!(
      loaded
        .unknown
        .contains(&"notification.timout_ms".to_string())
    );

    // bars come whole from the last layer that has any
    let bars = write("d.toml", "[bar.side]\nposition = \"left\"\n");
    let loaded = read_files(&[a.clone(), bars.clone()]).unwrap();
    assert_eq!(loaded.config.bar.keys().collect::<Vec<_>>(), ["side"]);
    let more = write("e.toml", "[bar.main]\n[bar.side]\nposition = \"left\"\n");
    let loaded = read_files(&[bars, more]).unwrap();
    assert_eq!(
      loaded.config.bar.keys().collect::<Vec<_>>(),
      ["main", "side"]
    );

    // one idle behavior changed keeps the defaults, its own fields too
    let idle = write("f.toml", "[idle.behavior.lock]\nenabled = false\n");
    let loaded = read_files(&[idle]).unwrap();
    let mut defaults = Config::default().idle;
    defaults.behavior.get_mut("lock").unwrap().enabled = false;
    assert_eq!(loaded.config.idle, defaults);
    let custom = write("g.toml", "[idle.behavior.notify]\ntimeout = 30\n");
    let loaded = read_files(&[custom]).unwrap();
    assert_eq!(loaded.config.idle.behavior.len(), 4);
    assert_eq!(loaded.config.idle.behavior["notify"].timeout, 30.);

    let bad = write("c.toml", "[osd]\nhide_delay_ms = \"soon\"\n");
    let e = read_files(&[a, b, bad.clone()]).unwrap_err().to_string();
    assert!(e.contains("c.toml"), "{e}");

    fs::remove_dir_all(dir).unwrap();
  }
}
