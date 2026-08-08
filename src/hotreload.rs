//! Debug-only: watches `ui/` and tells the main loop to recompile+swap the bar UI on change.
//! Only compiled when `--features hot-reload` is on (see `ui_loader`/`bar_ui` for the swap side).

use notify::Watcher as _;

use crate::events::{ShellEvent, ShellSender};

pub fn spawn(tx: ShellSender) {
  std::thread::spawn(move || {
    let (raw_tx, raw_rx) = std::sync::mpsc::channel();
    let mut watcher = match notify::recommended_watcher(move |res| {
      let _ = raw_tx.send(res);
    }) {
      Ok(w) => w,
      Err(e) => {
        tracing::error!("failed to create ui/ file watcher: {e}");
        return;
      }
    };

    let ui_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui");
    if let Err(e) = watcher.watch(&ui_dir, notify::RecursiveMode::NonRecursive) {
      tracing::error!("failed to watch {}: {e}", ui_dir.display());
      return;
    }
    tracing::info!("hot-reload: watching {}", ui_dir.display());

    for res in raw_rx {
      match res {
        Ok(event)
          if matches!(
            event.kind,
            notify::EventKind::Modify(_) | notify::EventKind::Create(_)
          ) =>
        {
          let _ = tx.send(ShellEvent::UiChanged);
        }
        Ok(_) => {}
        Err(e) => tracing::warn!("ui/ file watcher error: {e}"),
      }
    }
  });
}
