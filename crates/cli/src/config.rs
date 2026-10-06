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
