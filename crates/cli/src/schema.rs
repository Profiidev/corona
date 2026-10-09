use std::{fs, path::PathBuf};

use anyhow::Result;
use clap::Subcommand;

/// JSON schemas for editors, of the files plugins and plugin sources ship
#[derive(Subcommand)]
pub enum SchemaCommands {
  /// The schema of a plugin's `plugin.toml`
  Plugin {
    /// Write it to this file instead of printing it
    #[arg(short, long)]
    output: Option<PathBuf>,
  },
  /// The schema of a plugin source's `catalog.toml`
  Catalog {
    /// Write it to this file instead of printing it
    #[arg(short, long)]
    output: Option<PathBuf>,
  },
}

impl SchemaCommands {
  pub fn execute(self) {
    if let Err(e) = self.run() {
      eprintln!("error: {e:#}");
      std::process::exit(1);
    }
  }

  fn run(self) -> Result<()> {
    let (schema, output) = match self {
      SchemaCommands::Plugin { output } => (corona_script::plugin::plugin_schema(), output),
      SchemaCommands::Catalog { output } => (corona_script::plugin::catalog_schema(), output),
    };
    let text = serde_json::to_string_pretty(&schema)? + "\n";
    match output {
      Some(path) => fs::write(path, text)?,
      None => print!("{text}"),
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn writes_each_schema() {
    let dir = tempfile::tempdir().unwrap();
    let (plugin, catalog) = (dir.path().join("p.json"), dir.path().join("c.json"));
    SchemaCommands::Plugin {
      output: Some(plugin.clone()),
    }
    .run()
    .unwrap();
    SchemaCommands::Catalog {
      output: Some(catalog.clone()),
    }
    .run()
    .unwrap();
    let read = |path: &PathBuf| -> serde_json::Value {
      serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
    };
    assert_eq!(read(&plugin)["title"], "ManifestFile");
    assert_eq!(read(&catalog)["title"], "CatalogFile");
    assert!(
      SchemaCommands::Plugin {
        output: Some(dir.path().join("missing/p.json"))
      }
      .run()
      .is_err()
    );
  }
}
