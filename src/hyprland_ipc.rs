//! Hyprland workspace/active-window state, pushed into the shell over a calloop channel.
//!
//! ponytail: every event just re-fetches full state via `hyprctl` (through hyprland-rs) instead
//! of tracking/diffing the event payloads incrementally. Simple, always-correct, and cheap enough
//! at the rate workspace/window events actually fire. Revisit only if hyprctl round-trips show up
//! in a profile.

use hyprland::data::{Client, Workspace, Workspaces};
use hyprland::event_listener::EventListener;
use hyprland::shared::{HyprData, HyprDataActive, HyprDataActiveOptional};

use crate::events::{ShellEvent, ShellSender, WorkspaceInfo};

pub fn spawn(tx: ShellSender) {
  std::thread::spawn(move || {
    if let Err(e) = run(tx) {
      tracing::error!("Hyprland event listener stopped: {e:#}");
    }
  });
}

fn send_workspaces(tx: &ShellSender) {
  let Ok(workspaces) = Workspaces::get() else {
    return;
  };
  let active_id = Workspace::get_active().ok().map(|w| w.id);
  let infos = workspaces
    .iter()
    .map(|w| WorkspaceInfo {
      id: w.id,
      name: w.name.clone(),
      active: Some(w.id) == active_id,
    })
    .collect();
  let _ = tx.send(ShellEvent::Workspaces(infos));
}

fn send_active_window(tx: &ShellSender) {
  let title = Client::get_active().ok().flatten().map(|c| c.title);
  let _ = tx.send(ShellEvent::ActiveWindowTitle(title));
}

fn run(tx: ShellSender) -> anyhow::Result<()> {
  // Initial snapshot so the bar isn't empty until the first event fires.
  send_workspaces(&tx);
  send_active_window(&tx);

  let mut listener = EventListener::new();

  let t = tx.clone();
  listener.add_workspace_changed_handler(move |_| send_workspaces(&t));
  let t = tx.clone();
  listener.add_workspace_added_handler(move |_| send_workspaces(&t));
  let t = tx.clone();
  listener.add_workspace_deleted_handler(move |_| send_workspaces(&t));
  let t = tx.clone();
  listener.add_active_window_changed_handler(move |_| send_active_window(&t));

  // Blocks forever reading the Hyprland IPC event socket.
  listener.start_listener()?;
  Ok(())
}
