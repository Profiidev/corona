//! Hyprland IPC: the event listener thread and the request helpers.
//!
//! Named `hypr` rather than `hyprland` so `use hyprland::..` inside it keeps
//! resolving to the crate of that name.

use hyprland::{
  data::Workspaces,
  dispatch::{Dispatch, DispatchType, WorkspaceIdentifierWithSpecial},
  event_listener::EventListener,
  shared::HyprData,
};

use crate::event::{ShellEvent, ShellSender, WorkspaceEvent};

pub fn workspace_names() -> hyprland::Result<Vec<String>> {
  Ok(Workspaces::get()?.into_iter().map(|w| w.name).collect())
}

pub fn dispatch_workspace(name: &str) -> hyprland::Result<()> {
  Dispatch::call(DispatchType::Workspace(
    WorkspaceIdentifierWithSpecial::Name(name),
  ))
}

/// The listener blocks on Hyprland's socket, so it gets an OS thread of its
/// own and reports through `tx`.
pub fn spawn_listener(tx: ShellSender) {
  std::thread::spawn(move || {
    if let Err(e) = run(tx) {
      tracing::error!("Hyprland event listener stopped: {e:#}");
    }
  });
}

fn run(tx: ShellSender) -> anyhow::Result<()> {
  let mut listener = EventListener::new();

  let t = tx.clone();
  listener.add_workspace_changed_handler(move |e| {
    let _ = t.try_send(ShellEvent::Workspace(WorkspaceEvent::Changed {
      name: e.name,
      id: e.id,
    }));
  });

  let t = tx.clone();
  listener.add_workspace_added_handler(move |e| {
    let _ = t.try_send(ShellEvent::Workspace(WorkspaceEvent::Added {
      name: e.name,
      id: e.id,
    }));
  });

  let t = tx.clone();
  listener.add_workspace_deleted_handler(move |e| {
    let _ = t.try_send(ShellEvent::Workspace(WorkspaceEvent::Deleted {
      name: e.name,
      id: e.id,
    }));
  });

  listener.start_listener()?;
  Ok(())
}
