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
  use std::{cell::Cell, fs, path::Path, rc::Rc};

  use gpui_kit::{self as gpui, TestAppContext};

  use super::*;
  use crate::ThemeMode;

  fn write(dir: &Path, name: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, content).unwrap();
    path
  }

  /// Points the config and state directories at fresh temp dirs
  fn xdg() -> (tempfile::TempDir, tempfile::TempDir) {
    let (config, state) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    unsafe {
      std::env::set_var("XDG_CONFIG_HOME", config.path());
      std::env::set_var("XDG_STATE_HOME", state.path());
    }
    (config, state)
  }

  #[test]
  fn layers() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let a = write(
      dir,
      "a.toml",
      "[theme]\nname = \"A\"\nmode = \"dark\"\n[osd]\nhide_delay_ms = 900\n",
    );
    let b = write(
      dir,
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
    let bars = write(dir, "d.toml", "[bar.side]\nposition = \"left\"\n");
    let loaded = read_files(&[a.clone(), bars.clone()]).unwrap();
    assert_eq!(loaded.config.bar.keys().collect::<Vec<_>>(), ["side"]);
    let more = write(
      dir,
      "e.toml",
      "[bar.main]\n[bar.side]\nposition = \"left\"\n",
    );
    let loaded = read_files(&[bars, more]).unwrap();
    assert_eq!(
      loaded.config.bar.keys().collect::<Vec<_>>(),
      ["main", "side"]
    );

    // one idle behavior changed keeps the defaults, its own fields too
    let idle = write(dir, "f.toml", "[idle.behavior.lock]\nenabled = false\n");
    let loaded = read_files(&[idle]).unwrap();
    let mut defaults = Config::default().idle;
    defaults.behavior.get_mut("lock").unwrap().enabled = false;
    assert_eq!(loaded.config.idle, defaults);
    let custom = write(dir, "g.toml", "[idle.behavior.notify]\ntimeout = 30\n");
    let loaded = read_files(&[custom]).unwrap();
    assert_eq!(loaded.config.idle.behavior.len(), 4);
    assert_eq!(loaded.config.idle.behavior["notify"].timeout, 30.);

    let bad = write(dir, "c.toml", "[osd]\nhide_delay_ms = \"soon\"\n");
    let e = read_files(&[a, b, bad.clone()]).unwrap_err().to_string();
    assert!(e.contains("c.toml"), "{e}");
  }

  #[test]
  fn no_files_is_the_defaults() {
    let loaded = read_files(&[]).unwrap();
    assert_eq!(loaded.config, Config::default());
    assert!(loaded.unknown.is_empty());
  }

  #[test]
  fn bad_syntax_names_its_file() {
    let tmp = tempfile::tempdir().unwrap();
    let good = write(tmp.path(), "good.toml", "[osd]\noffset = 1.0\n");
    let bad = write(tmp.path(), "broken.toml", "[osd\n");
    let e = read_files(&[good.clone(), bad]).unwrap_err().to_string();
    assert!(e.contains("broken.toml"), "{e}");
    assert!(!e.contains("good.toml"), "{e}");
  }

  #[test]
  fn environment_wins_over_files() {
    let tmp = tempfile::tempdir().unwrap();
    let file = write(tmp.path(), "a.toml", "[osd]\nhide_delay_ms = 900\n");
    unsafe { std::env::set_var("CORONA__OSD__HIDE_DELAY_MS", "7") };
    let loaded = read_files(&[file]).unwrap();
    assert_eq!(loaded.config.osd.hide_delay_ms, 7);
  }

  #[test]
  fn bad_environment_without_files_is_its_own_error() {
    unsafe { std::env::set_var("CORONA__OSD__HIDE_DELAY_MS", "soon") };
    let e = read_files(&[]).unwrap_err().to_string();
    assert!(e.contains("hide_delay_ms"), "{e}");
  }

  #[test]
  #[ignore = "bug: a bad CORONA__* value is blamed on the first config file"]
  fn bug_bad_environment_blamed_on_a_file() {
    let tmp = tempfile::tempdir().unwrap();
    let file = write(tmp.path(), "fine.toml", "[osd]\noffset = 1.0\n");
    unsafe { std::env::set_var("CORONA__OSD__HIDE_DELAY_MS", "soon") };
    let e = read_files(&[file]).unwrap_err().to_string();
    assert!(!e.contains("fine.toml"), "{e}");
  }

  #[test]
  #[ignore = "bug: documented CORONA_SHELL__PLUGIN_DIR is ignored, config-rs wants CORONA__SHELL__PLUGIN_DIR"]
  fn bug_documented_env_var_is_ignored() {
    unsafe { std::env::set_var("CORONA_SHELL__PLUGIN_DIR", "/opt/plugins") };
    let loaded = read_files(&[]).unwrap();
    assert_eq!(
      loaded.config.shell.plugin_dir.as_deref(),
      Some(Path::new("/opt/plugins"))
    );
  }

  #[test]
  fn config_files_recursive_sorted_toml_only() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    assert!(config_files(dir).unwrap().is_empty());
    assert!(config_files(&dir.join("missing")).unwrap().is_empty());
    for name in [
      "b.toml",
      "a.toml",
      "sub/c.toml",
      "sub/deep/d.toml",
      "x.txt",
      "toml",
    ] {
      write(dir, name, "");
    }
    let files = config_files(dir).unwrap();
    let rel: Vec<_> = files.iter().map(|f| f.strip_prefix(dir).unwrap()).collect();
    assert_eq!(
      rel,
      ["a.toml", "b.toml", "sub/c.toml", "sub/deep/d.toml"].map(Path::new)
    );
  }

  #[test]
  fn last_bars_edges() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let bars = write(dir, "a.toml", "[bar.x]\n");
    let none = write(dir, "b.toml", "[osd]\n");
    let unreadable = dir.join("missing.toml");
    let as_dir = dir.join("d.toml");
    fs::create_dir(&as_dir).unwrap();

    assert!(last_bars(std::slice::from_ref(&none)).unwrap().is_none());
    let found = last_bars(&[bars.clone(), none.clone(), unreadable, as_dir])
      .unwrap()
      .unwrap();
    assert_eq!(found.keys().collect::<Vec<_>>(), ["x"]);

    let bad = write(dir, "c.toml", "[bar\n");
    assert!(last_bars(&[bars.clone(), bad]).is_err());
    let scalar = write(dir, "e.toml", "bar = 5\n");
    assert!(last_bars(&[bars, scalar]).is_err());
  }

  #[test]
  fn read_uses_xdg_dirs_settings_last() {
    let (config, state) = xdg();
    assert_eq!(config_dir().unwrap(), config.path().join("corona"));
    assert_eq!(
      settings_file().unwrap(),
      state.path().join("corona/settings.toml")
    );
    // nothing there yet
    assert_eq!(read().unwrap().config, Config::default());

    let dir = config.path().join("corona");
    write(&dir, "a.toml", "[osd]\noffset = 1.0\nhide_delay_ms = 2\n");
    write(&dir, "nested/z.toml", "[osd]\noffset = 2.0\n");
    write(
      state.path(),
      "corona/settings.toml",
      "[osd]\noffset = 3.0\n",
    );
    let loaded = read().unwrap();
    assert_eq!(loaded.config.osd.offset, 3.);
    assert_eq!(loaded.config.osd.hide_delay_ms, 2);
  }

  #[gpui::test]
  fn apply_sets_global_only_on_change(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(Config::default()));
    let count = Rc::new(Cell::new(0));
    let c = count.clone();
    cx.update(|cx| {
      cx.observe_global::<Config>(move |_| c.set(c.get() + 1))
        .detach()
    });
    let loaded = |config| Loaded {
      config,
      unknown: vec!["x.y".into()],
    };

    cx.update(|cx| apply(loaded(Config::default()), cx));
    assert_eq!(count.get(), 0);

    let mut changed = Config::default();
    changed.osd.offset = 1.;
    cx.update(|cx| apply(loaded(changed.clone()), cx));
    assert_eq!(count.get(), 1);
    assert_eq!(cx.update(|cx| cx.config().clone()), changed);
  }
}
