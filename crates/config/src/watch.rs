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
