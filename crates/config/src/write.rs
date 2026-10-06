use std::fs;

use anyhow::{Context, Result};
use gpui_kit::App;
use toml::Value;

use crate::{
  Config, ConfigProvider,
  read::{apply, config_dir, config_files, read, read_files, settings_file},
};

/// Changes settings from inside the shell, from IPC or the settings app.
///
/// Only what differs from the user's config files is written, to the settings
/// file, so their files are never touched (Nix keeps them read-only) and a
/// setting that goes back to their value leaves the settings file again.
pub fn update(cx: &mut App, edit: impl FnOnce(&mut Config)) -> Result<()> {
  let mut config = cx.config().clone();
  edit(&mut config);

  let base = read_files(&config_files(&config_dir()?)?)?.config;
  let mut overrides = diff(&Value::try_from(&base)?, &Value::try_from(&config)?)
    .unwrap_or_else(|| Value::Table(Default::default()));
  // bars are read whole from one layer, so they are written whole too
  if let Value::Table(overrides) = &mut overrides
    && overrides.contains_key("bar")
  {
    overrides.insert("bar".to_string(), Value::try_from(&config.bar)?);
  }

  let file = settings_file()?;
  fs::create_dir_all(file.parent().context("settings file has no directory")?)?;
  // written whole then renamed over, so a reader never sees half of it
  let tmp = file.with_extension("toml.tmp");
  fs::write(&tmp, toml::to_string_pretty(&overrides)?)?;
  fs::rename(&tmp, &file)?;

  apply(read()?, cx);
  Ok(())
}

/// What `new` sets that `base` does not, down to single values; tables merge,
/// anything else is replaced whole.
fn diff(base: &Value, new: &Value) -> Option<Value> {
  match (base, new) {
    (Value::Table(base), Value::Table(new)) => {
      let table: toml::Table = new
        .iter()
        .filter_map(|(key, value)| {
          let changed = match base.get(key) {
            Some(old) => diff(old, value)?,
            None => value.clone(),
          };
          Some((key.clone(), changed))
        })
        .collect();
      (!table.is_empty()).then_some(Value::Table(table))
    }
    _ => (base != new).then(|| new.clone()),
  }
}

#[cfg(test)]
mod tests {
  use super::diff;

  #[test]
  fn only_changes() {
    let base: toml::Value = toml::from_str("a = 1\n[t]\nb = 2\nc = [1, 2]\n").unwrap();
    let new: toml::Value = toml::from_str("a = 1\n[t]\nb = 3\nc = [1, 2]\nd = true\n").unwrap();
    let changed = diff(&base, &new).unwrap();
    assert_eq!(changed, toml::from_str("[t]\nb = 3\nd = true\n").unwrap());
    assert_eq!(diff(&base, &base), None);
  }
}
