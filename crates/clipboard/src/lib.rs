//! Clipboard history: a watcher on the data control protocol fills a SQLite store, the
//! launcher reads it; pasting presses Ctrl+V on a virtual keyboard

pub mod paste;
pub mod placeholders;
pub mod store;
pub mod watch;

use std::{rc::Rc, sync::Arc};

use anyhow::Result;
use corona_config::{ConfigProvider, launcher::ClipboardConfig, observe_section};
use corona_utils::Buses;
use gpui_kit::{App, AppContext, Entity, Global};

pub use store::{Entry, Kind, Store};
use watch::{Captured, MAX_TEXT};

#[derive(Clone)]
pub struct ClipboardHistory {
  store: Rc<Store>,
  /// Bumped on every change, observe it to follow the history
  pub revision: Entity<u64>,
}

impl Global for ClipboardHistory {}

pub trait ClipboardExt {
  /// None when the history could not be opened
  fn clipboard_history(&self) -> Option<&ClipboardHistory>;
}

impl ClipboardExt for App {
  fn clipboard_history(&self) -> Option<&ClipboardHistory> {
    self.try_global::<ClipboardHistory>()
  }
}

/// Opens the history and watches the clipboard
pub fn init(cx: &mut App, _: &Buses) {
  if let Err(e) = start(cx) {
    tracing::warn!("clipboard history is off: {e:#}");
  }
}

/// Opens the history and starts watching the clipboard
fn start(cx: &mut App) -> Result<()> {
  let store = Store::open(&Store::default_dir()?)?;
  let days = cx.config().launcher.clipboard.image_retention_days;
  if let Err(e) = store.prune_images(days) {
    tracing::warn!("clipboard: pruning images: {e:#}");
  }
  let ClipboardHistory { store, revision } = ClipboardHistory::install(store, cx);

  let wanted = Arc::new(watch::Wanted::default());
  share(&wanted, &cx.config().launcher.clipboard);
  observe_section(cx, |c| &c.launcher.clipboard, {
    let wanted = wanted.clone();
    move |config, _| share(&wanted, config)
  });
  let (tx, rx) = flume::unbounded();
  std::thread::Builder::new()
    .name("clipboard".into())
    .spawn(move || {
      if let Err(e) = watch::run(tx, wanted) {
        tracing::warn!("clipboard watcher stopped: {e:#}");
      }
    })?;
  cx.spawn(async move |cx| {
    let added = move |result: Result<i64>, cx: &mut App| match result {
      Ok(_) => revision.update(cx, |r, cx| {
        *r += 1;
        cx.notify();
      }),
      Err(e) => tracing::warn!("clipboard: {e:#}"),
    };
    while let Ok(Captured { mime, bytes }) = rx.recv_async().await {
      let config = cx.update(|cx| cx.config().launcher.clipboard.clone());
      if !config.enabled {
        continue;
      }
      if mime.starts_with("image/") {
        if !config.images || bytes.len() as f32 > config.image_max_mb * 1024. * 1024. {
          continue;
        }
        // hashing and writing a big image must not stall the UI thread
        let (dir, size, kind) = (store.dir().to_path_buf(), bytes.len() as u64, mime.clone());
        let file = cx
          .background_executor()
          .spawn(async move { Store::write_image(&dir, &bytes, &kind) })
          .await;
        cx.update(|cx| {
          added(
            file.and_then(|(hash, path)| store.add_image_file(hash, &path, &mime, size)),
            cx,
          )
        });
        continue;
      }
      // Longer text is not kept, storing it would stall the UI thread
      if bytes.len() > MAX_TEXT {
        continue;
      }
      let Ok(text) = String::from_utf8(bytes) else {
        continue;
      };
      if !text.trim().is_empty() {
        cx.update(|cx| added(store.add_text(&text, &mime), cx));
      }
    }
  })
  .detach();
  Ok(())
}

/// Hands the watcher what the config lets the history take
fn share(wanted: &watch::Wanted, config: &ClipboardConfig) {
  wanted.set(
    config.enabled,
    config.images,
    (config.image_max_mb * 1024. * 1024.) as u64,
  );
}

impl ClipboardHistory {
  /// Makes `store` the history, without watching the clipboard
  pub fn install(store: Store, cx: &mut App) -> Self {
    let history = ClipboardHistory {
      store: Rc::new(store),
      revision: cx.new(|_| 0u64),
    };
    cx.set_global(history.clone());
    history
  }

  pub fn list(&self, query: &str, kind: Option<Kind>, limit: usize) -> Vec<Entry> {
    self.store.list(query, kind, limit).unwrap_or_else(|e| {
      tracing::warn!("clipboard: {e:#}");
      Vec::new()
    })
  }

  pub fn get(&self, id: i64) -> Option<Entry> {
    self.store.get(id).unwrap_or_else(|e| {
      tracing::warn!("clipboard: {e:#}");
      None
    })
  }

  /// The `offset`th newest text, for `{clipboard offset=1}`
  pub fn nth_text(&self, offset: usize) -> Option<String> {
    self.store.nth_text(offset).unwrap_or_else(|e| {
      tracing::warn!("clipboard: {e:#}");
      None
    })
  }

  fn changed(&self, result: Result<()>, cx: &mut App) -> Result<()> {
    result?;
    self.revision.update(cx, |r, cx| {
      *r += 1;
      cx.notify();
    });
    Ok(())
  }

  pub fn set_pinned(&self, id: i64, pinned: bool, cx: &mut App) -> Result<()> {
    self.changed(self.store.set_pinned(id, pinned), cx)
  }

  pub fn touch(&self, id: i64, cx: &mut App) -> Result<()> {
    self.changed(self.store.touch(id), cx)
  }

  pub fn delete(&self, id: i64, cx: &mut App) -> Result<()> {
    self.changed(self.store.delete(id), cx)
  }

  /// Everything but the pinned entries; how many went
  pub fn clear(&self, cx: &mut App) -> Result<usize> {
    let n = self.store.clear()?;
    self.changed(Ok(()), cx)?;
    Ok(n)
  }
}
