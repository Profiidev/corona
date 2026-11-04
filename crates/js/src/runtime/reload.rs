//! Hot reload: polls the sources of a plugin directory and reports a change once it held
//! still for a poll.

use std::{
  path::{Path, PathBuf},
  time::{Duration, SystemTime},
};

use anyhow::Result;
use gpui_kit::{AnyWindowHandle, App, Context, Entity, Task, Window};

use crate::ScriptView;

pub const POLL: Duration = Duration::from_millis(250);

/// Source files below `dir` with their modification time and size, sorted
type Stamp = Vec<(PathBuf, SystemTime, u64)>;

fn is_source(path: &Path) -> bool {
  let name = path
    .file_name()
    .and_then(|n| n.to_str())
    .unwrap_or_default();
  !name.ends_with(".d.ts")
    && matches!(
      path.extension().and_then(|e| e.to_str()),
      Some("js" | "mjs" | "ts" | "mts")
    )
}

/// Deep enough for any plugin tree, shallow enough to end a symlink loop
const MAX_DEPTH: usize = 32;

fn stamp(dir: &Path, depth: usize, out: &mut Stamp) {
  if depth > MAX_DEPTH {
    return;
  }
  let Ok(entries) = std::fs::read_dir(dir) else {
    return;
  };
  for entry in entries.flatten() {
    let path = entry.path();
    let name = entry.file_name();
    if name.to_string_lossy().starts_with('.') || name == "node_modules" {
      continue;
    }
    // follows symlinks, unlike `entry.metadata()`
    let Ok(meta) = std::fs::metadata(&path) else {
      continue;
    };
    if meta.is_dir() {
      stamp(&path, depth + 1, out);
    } else if is_source(&path) {
      out.push((
        path,
        meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        meta.len(),
      ));
    }
  }
}

fn scan(dir: &Path) -> Stamp {
  let mut out = Vec::new();
  stamp(dir, 0, &mut out);
  out.sort();
  out
}

/// Calls `on_change` after a source below `dir` changed, until the task is dropped.
/// ponytail: scans on the foreground every poll, fine for plugin-sized trees
pub fn watch(
  dir: PathBuf,
  cx: &mut App,
  mut on_change: impl FnMut(&mut App) + 'static,
) -> Task<()> {
  let mut last = scan(&dir);
  cx.spawn(async move |cx| {
    let mut pending = None;
    loop {
      cx.background_executor().timer(POLL).await;
      let now = scan(&dir);
      if now == last {
        pending = None;
      } else if pending.as_ref() == Some(&now) {
        last = now;
        pending = None;
        cx.update(|cx| on_change(cx));
      } else {
        pending = Some(now);
      }
    }
  })
}

/// Rebuilds `view` in `window` with `make` whenever a source below `dir` changes. A failed
/// build is logged and keeps the old view.
pub fn reload_view(
  view: &Entity<ScriptView>,
  window: AnyWindowHandle,
  dir: PathBuf,
  make: impl Fn(&mut Window, &mut Context<ScriptView>) -> Result<ScriptView> + 'static,
  cx: &mut App,
) -> Task<()> {
  let view = view.downgrade();
  watch(dir, cx, move |cx| {
    let Some(view) = view.upgrade() else {
      return;
    };
    let _ = window.update(cx, |_, window, cx| {
      view.update(cx, |view, cx| match make(window, cx) {
        Ok(new) => {
          *view = new;
          cx.notify();
        }
        Err(e) => tracing::warn!("reload failed, keeping the last view: {e:#}"),
      })
    });
  })
}
