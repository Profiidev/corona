//! Hot reload: polls the sources of a plugin directory and reports a change once it held
//! still for a poll.

use std::{
  path::{Path, PathBuf},
  time::{Duration, Instant, SystemTime},
};

use anyhow::Result;
use gpui_kit::{AnyWindowHandle, App, Context, Entity, Task, Window};

use crate::{ScriptView, runtime::limits};

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
    // directory symlinks are not followed: loops and links out of the plugin would blow up
    // the walk. Symlinked files still count, `metadata` follows them.
    let Ok(ty) = entry.file_type() else {
      continue;
    };
    if ty.is_dir() {
      stamp(&path, depth + 1, out);
      continue;
    }
    let Ok(meta) = std::fs::metadata(&path) else {
      continue;
    };
    if meta.is_file() && is_source(&path) {
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

/// Calls `on_change` after a source below `dir` changed or plugin `id` hit a limit, until
/// the task is dropped.
/// ponytail: scans on the foreground every poll, fine for plugin-sized trees
pub fn watch(
  dir: PathBuf,
  id: String,
  cx: &mut App,
  mut on_change: impl FnMut(&mut App) + 'static,
) -> Task<()> {
  let mut last = scan(&dir);
  cx.spawn(async move |cx| {
    let mut pending = None;
    let mut trips = Vec::new();
    loop {
      cx.background_executor().timer(POLL).await;
      if let Some(trip) = limits::take_tripped(&id) {
        let now = Instant::now();
        trips.retain(|t| now.duration_since(*t) < limits::WINDOW);
        trips.push(now);
        let give_up = trips.len() >= limits::MAX_TRIPS;
        cx.update(|cx| {
          tracing::warn!("plugin `{id}` hit a limit ({trip:?}), giving up: {give_up}");
          if let Some(hook) = cx.try_global::<limits::TripHook>().map(|h| h.0.clone()) {
            hook(&id, trip, give_up, cx);
          }
          if !give_up {
            on_change(cx);
          }
        });
        if give_up {
          break;
        }
        continue;
      }
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
  id: String,
  make: impl Fn(&mut Window, &mut Context<ScriptView>) -> Result<ScriptView> + 'static,
  cx: &mut App,
) -> Task<()> {
  let view = view.downgrade();
  watch(dir, id, cx, move |cx| {
    let Some(view) = view.upgrade() else {
      return;
    };
    let _ = window.update(cx, |_, window, cx| {
      view.update(cx, |view, cx| match make(window, cx) {
        Ok(new) => {
          crate::view::drop_bindings(cx, view.owner());
          *view = new;
          cx.notify();
        }
        Err(e) => tracing::warn!("reload failed, keeping the last view: {e:#}"),
      })
    });
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn scan_skips_directory_symlinks() {
    let dir = std::env::temp_dir().join(format!("corona-scan-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.js"), "").unwrap();
    std::os::unix::fs::symlink(".", dir.join("self1")).unwrap();
    std::os::unix::fs::symlink(".", dir.join("self2")).unwrap();
    std::os::unix::fs::symlink(dir.join("a.js"), dir.join("b.js")).unwrap();
    let found = scan(&dir);
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(found.len(), 2);
  }
}
