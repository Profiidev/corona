use std::{fs, time::Duration};

use anyhow::{Context, Result};
use gpui_kit::App;
use notify::{RecursiveMode, Watcher};

use crate::read::{apply, config_dir, read, settings_file};

/// Editors save in bursts of events, one read covers them.
const DEBOUNCE: Duration = Duration::from_millis(150);

/// Re-reads the settings whenever a file in the config directory or the
/// settings file changes. A file that does not parse leaves the settings as they
/// are and goes to `on_error`.
///
/// The directories are watched rather than the files: home-manager replaces its
/// symlink and editors save by renaming, both of which a file watch misses.
pub fn watch(cx: &mut App, on_error: impl Fn(String, &mut App) + 'static) -> Result<()> {
  let (tx, rx) = flume::unbounded();
  let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
    if event.is_ok_and(|e| !e.kind.is_access()) {
      let _ = tx.send(());
    }
  })?;
  let settings = settings_file()?;
  let state_dir = settings
    .parent()
    .context("settings file has no directory")?;
  for dir in [config_dir()?.as_path(), state_dir] {
    fs::create_dir_all(dir)?;
    watcher.watch(dir, RecursiveMode::Recursive)?;
  }

  cx.spawn(async move |cx| {
    let _watcher = watcher;
    while rx.recv_async().await.is_ok() {
      cx.background_executor().timer(DEBOUNCE).await;
      rx.drain();
      let loaded = cx.background_executor().spawn(async { read() }).await;
      cx.update(|cx| match loaded {
        Ok(loaded) => apply(loaded, cx),
        Err(e) => {
          tracing::error!("Failed to reload config: {e:#}");
          on_error(format!("{e:#}"), cx);
        }
      });
    }
  })
  .detach();
  Ok(())
}

#[cfg(test)]
mod tests {
  use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
  };

  use gpui_kit::{self as gpui, TestAppContext};

  use super::*;
  use crate::{Config, ConfigProvider};

  /// Runs the app in real time until `done`: file events come from notify's own
  /// thread. Fake time moves past the debounce on every turn.
  fn wait_until(cx: &mut TestAppContext, mut done: impl FnMut(&mut TestAppContext) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
      cx.run_until_parked();
      cx.executor().advance_clock(DEBOUNCE * 2);
      cx.run_until_parked();
      if done(cx) {
        return;
      }
      assert!(Instant::now() < deadline, "timed out waiting");
      std::thread::sleep(Duration::from_millis(5));
    }
  }

  #[gpui::test]
  fn reloads_on_change_and_keeps_config_on_error(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (config, state) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    unsafe {
      std::env::set_var("XDG_CONFIG_HOME", config.path());
      std::env::set_var("XDG_STATE_HOME", state.path());
    }
    let errors = Rc::new(RefCell::new(Vec::new()));
    let e = errors.clone();
    cx.update(|cx| {
      cx.set_global(Config::default());
      watch(cx, move |error, _| e.borrow_mut().push(error)).unwrap();
    });
    let dir = config.path().join("corona");
    assert!(dir.is_dir() && state.path().join("corona").is_dir());

    fs::write(dir.join("a.toml"), "[osd]\noffset = 9.0\n").unwrap();
    wait_until(cx, |cx| cx.update(|cx| cx.config().osd.offset) == 9.);
    assert!(errors.borrow().is_empty());

    fs::write(dir.join("b.toml"), "[osd\n").unwrap();
    wait_until(cx, |_| !errors.borrow().is_empty());
    assert!(
      errors.borrow()[0].contains("b.toml"),
      "{:?}",
      errors.borrow()
    );
    assert_eq!(cx.update(|cx| cx.config().osd.offset), 9.);

    // the settings file is watched too
    fs::remove_file(dir.join("b.toml")).unwrap();
    fs::write(
      state.path().join("corona/settings.toml"),
      "[osd]\noffset = 3.0\n",
    )
    .unwrap();
    wait_until(cx, |cx| cx.update(|cx| cx.config().osd.offset) == 3.);
  }
}
