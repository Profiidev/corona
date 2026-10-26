//! Clipboard history: a watcher on the data control protocol fills a SQLite store, the
//! launcher reads it; pasting presses Ctrl+V on a virtual keyboard

pub mod paste;
pub mod store;
pub mod watch;

use std::rc::Rc;

use anyhow::Result;
use corona_config::ConfigProvider;
use gpui_kit::{App, AppContext, Entity, Global};

pub use store::{Entry, Kind, Store};

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

/// Opens the history and starts watching the clipboard
pub fn init(cx: &mut App) -> Result<()> {
  let store = Store::open(&Store::default_dir()?)?;
  let days = cx.config().launcher.clipboard.image_retention_days;
  if let Err(e) = store.prune_images(days) {
    tracing::warn!("clipboard: pruning images: {e:#}");
  }
  let ClipboardHistory { store, revision } = ClipboardHistory::install(store, cx);

  let (tx, rx) = flume::unbounded();
  std::thread::Builder::new()
    .name("clipboard".into())
    .spawn(move || {
      if let Err(e) = watch::run(tx) {
        tracing::warn!("clipboard history is off: {e:#}");
      }
    })?;
  cx.spawn(async move |cx| {
    while let Ok(captured) = rx.recv_async().await {
      cx.update(|cx| {
        let config = cx.config().launcher.clipboard.clone();
        if !config.enabled {
          return;
        }
        let added = match captured.mime.starts_with("image/") {
          true if !config.images => return,
          true if captured.bytes.len() as f32 > config.image_max_mb * 1024. * 1024. => return,
          true => store.add_image(&captured.bytes, &captured.mime),
          false => match String::from_utf8(captured.bytes) {
            Ok(text) if !text.trim().is_empty() => store.add_text(&text, &captured.mime),
            _ => return,
          },
        };
        match added {
          Ok(_) => revision.update(cx, |r, cx| {
            *r += 1;
            cx.notify();
          }),
          Err(e) => tracing::warn!("clipboard: {e:#}"),
        }
      });
    }
  })
  .detach();
  Ok(())
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
    self.store.get(id).ok().flatten()
  }

  /// The `offset`th newest text, for `{clipboard offset=1}`
  pub fn nth_text(&self, offset: usize) -> Option<String> {
    self.store.nth_text(offset).ok().flatten()
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
