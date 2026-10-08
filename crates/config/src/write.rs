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
  use gpui_kit::{self as gpui, TestAppContext};
  use toml::Value;

  use super::*;
  use crate::{OsdConfig, ThemeMode, placement::Placement};

  fn value(s: &str) -> Value {
    toml::from_str(s).unwrap()
  }

  #[test]
  fn only_changes() {
    let base = value("a = 1\n[t]\nb = 2\nc = [1, 2]\n");
    let new = value("a = 1\n[t]\nb = 3\nc = [1, 2]\nd = true\n");
    let changed = diff(&base, &new).unwrap();
    assert_eq!(changed, value("[t]\nb = 3\nd = true\n"));
    assert_eq!(diff(&base, &base), None);
  }

  #[test]
  fn arrays_and_type_changes_replace_whole() {
    let base = value("c = [1, 2, 3]\n[t]\nx = 1\n");
    let new = value("c = [1, 2]\nt = 5\n");
    assert_eq!(diff(&base, &new).unwrap(), value("c = [1, 2]\nt = 5\n"));
    // a key only `base` has is not expressed
    assert_eq!(diff(&value("a = 1\nb = 2\n"), &value("a = 1\n")), None);
  }

  struct Env {
    config: tempfile::TempDir,
    state: tempfile::TempDir,
  }

  impl Env {
    /// Config and state dirs in temp dirs, the user's file holding `user`, and
    /// the [`Config`] global loaded from them
    fn new(cx: &mut TestAppContext, user: &str) -> Self {
      let env = Env {
        config: tempfile::tempdir().unwrap(),
        state: tempfile::tempdir().unwrap(),
      };
      unsafe {
        std::env::set_var("XDG_CONFIG_HOME", env.config.path());
        std::env::set_var("XDG_STATE_HOME", env.state.path());
      }
      let dir = env.config.path().join("corona");
      fs::create_dir_all(&dir).unwrap();
      fs::write(dir.join("user.toml"), user).unwrap();
      cx.update(crate::load).unwrap();
      env
    }

    fn settings(&self) -> Value {
      let dir = self.state.path().join("corona");
      let names: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
      assert_eq!(names, ["settings.toml"], "the temp file is renamed away");
      value(&fs::read_to_string(dir.join("settings.toml")).unwrap())
    }
  }

  fn config(cx: &mut TestAppContext) -> Config {
    cx.update(|cx| cx.config().clone())
  }

  #[gpui::test]
  fn writes_only_the_difference(cx: &mut TestAppContext) {
    let env = Env::new(cx, "[osd]\noffset = 5.0\n");
    cx.update(|cx| update(cx, |c| c.osd.hide_delay_ms = 42))
      .unwrap();
    assert_eq!(env.settings(), value("[osd]\nhide_delay_ms = 42\n"));
    let now = config(cx);
    assert_eq!(now.osd.hide_delay_ms, 42);
    assert_eq!(now.osd.offset, 5.);

    // back to the user's value leaves the settings file
    cx.update(|cx| {
      update(cx, |c| {
        c.osd.hide_delay_ms = OsdConfig::default().hide_delay_ms
      })
    })
    .unwrap();
    assert_eq!(env.settings(), value(""));
    // as does setting what the user's file already says
    cx.update(|cx| update(cx, |c| c.osd.offset = 5.)).unwrap();
    assert_eq!(env.settings(), value(""));
    assert_eq!(config(cx).osd.offset, 5.);
  }

  #[gpui::test]
  fn bars_are_written_whole(cx: &mut TestAppContext) {
    let env = Env::new(cx, "");
    cx.update(|cx| {
      update(cx, |c| {
        c.bar.get_mut("main").unwrap().thickness = 40.;
      })
    })
    .unwrap();
    let written = env.settings();
    let main = &written["bar"]["main"];
    assert_eq!(main["thickness"].as_float(), Some(40.));
    assert!(main.get("start").is_some() && main.get("position").is_some());
    assert_eq!(config(cx).bar["main"].thickness, 40.);

    // a bar can be dropped, too
    cx.update(|cx| {
      update(cx, |c| {
        c.bar.clear();
        c.bar.insert("side".into(), Default::default());
        c.bar.get_mut("side").unwrap().position = Placement::Left;
      })
    })
    .unwrap();
    assert_eq!(config(cx).bar.keys().collect::<Vec<_>>(), ["side"]);
  }

  #[gpui::test]
  #[ignore = "bug: diff cannot express removal, unsetting an option the user's file sets is lost"]
  fn bug_unsetting_a_user_option_is_lost(cx: &mut TestAppContext) {
    let _env = Env::new(cx, "[theme]\nmode = \"dark\"\n");
    assert_eq!(config(cx).theme.mode, Some(ThemeMode::Dark));
    cx.update(|cx| update(cx, |c| c.theme.mode = None)).unwrap();
    assert_eq!(config(cx).theme.mode, None);
  }

  #[gpui::test]
  fn bad_user_file_is_an_error_and_writes_nothing(cx: &mut TestAppContext) {
    let env = Env::new(cx, "");
    fs::write(env.config.path().join("corona/bad.toml"), "[osd\n").unwrap();
    assert!(cx.update(|cx| update(cx, |c| c.osd.offset = 1.)).is_err());
    assert!(!env.state.path().join("corona/settings.toml").exists());
    assert_eq!(config(cx), Config::default());
  }
}
