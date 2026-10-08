use std::path::PathBuf;

use anyhow::Result;
use clap::Subcommand;
use corona_config::Loaded;

#[derive(Subcommand)]
pub enum ConfigCommands {
  /// Check the settings without running the shell: the config files and the
  /// shell's settings file, as the shell reads them
  Validate {
    /// A file or directory to check instead
    path: Option<PathBuf>,
  },
  /// Print the settings in effect, every layer applied and every default
  /// filled in, as TOML
  Show {
    /// A file or directory to read instead
    path: Option<PathBuf>,
  },
}

impl ConfigCommands {
  pub fn execute(self) {
    let result = match self {
      ConfigCommands::Validate { path } => validate(path).map(|loaded| {
        warn_unknown(&loaded);
        println!("ok");
      }),
      ConfigCommands::Show { path } => validate(path).and_then(|loaded| {
        warn_unknown(&loaded);
        print!("{}", loaded.config.to_toml()?);
        Ok(())
      }),
    };
    if let Err(e) = result {
      eprintln!("error: {e:#}");
      std::process::exit(1);
    }
  }
}

fn warn_unknown(loaded: &Loaded) {
  for key in &loaded.unknown {
    eprintln!("warning: unknown setting {key}");
  }
}

fn validate(path: Option<PathBuf>) -> Result<Loaded> {
  match path {
    None => corona_config::read(),
    Some(dir) if dir.is_dir() => corona_config::read_files(&corona_config::config_files(&dir)?),
    Some(file) => {
      anyhow::ensure!(file.is_file(), "no such file: {}", file.display());
      corona_config::read_files(&[file])
    }
  }
}

#[cfg(test)]
mod tests {
  use std::fs;

  use super::*;

  #[test]
  fn validates_a_dir_or_a_file() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("sub")).unwrap();
    fs::write(dir.path().join("a.toml"), "[notification]\ntimout = 5\n").unwrap();
    fs::write(
      dir.path().join("sub/b.toml"),
      "[notification]\ntimeout_ms = 42\n",
    )
    .unwrap();

    let loaded = validate(Some(dir.path().into())).unwrap();
    // nested files count too
    assert_eq!(loaded.config.notification.timeout_ms, 42);
    assert_eq!(loaded.unknown, ["notification.timout"]);

    let loaded = validate(Some(dir.path().join("a.toml"))).unwrap();
    assert_ne!(loaded.config.notification.timeout_ms, 42);
  }

  #[test]
  fn bad_paths_and_values() {
    let dir = tempfile::tempdir().unwrap();
    let err = validate(Some(dir.path().join("missing.toml"))).unwrap_err();
    assert!(err.to_string().contains("no such file"));
    let bad = dir.path().join("bad.toml");
    fs::write(&bad, "[notification]\ntimeout_ms = \"soon\"\n").unwrap();
    assert!(validate(Some(bad)).is_err());
  }

  #[test]
  fn defaults_to_the_users_files() {
    let config = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    unsafe {
      std::env::set_var("XDG_CONFIG_HOME", config.path());
      std::env::set_var("XDG_STATE_HOME", state.path());
    }
    fs::create_dir(config.path().join("corona")).unwrap();
    fs::write(
      config.path().join("corona/a.toml"),
      "[notification]\ntimeout_ms = 1\n",
    )
    .unwrap();
    fs::create_dir(state.path().join("corona")).unwrap();
    // the settings file wins over the config files
    fs::write(
      state.path().join("corona/settings.toml"),
      "[notification]\ntimeout_ms = 2\n",
    )
    .unwrap();
    assert_eq!(validate(None).unwrap().config.notification.timeout_ms, 2);
  }
}
