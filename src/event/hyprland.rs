use hyprland::event_listener::EventListener;

use crate::{
  api::event::{ShellEvent, WorkspaceEvent},
  event::event_loop::ShellSender,
};

pub fn spawn(tx: ShellSender) {
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
    let _ = t.send(ShellEvent::Workspace(WorkspaceEvent::Changed {
      name: e.name,
      id: e.id,
    }));
  });

  let t = tx.clone();
  listener.add_workspace_added_handler(move |e| {
    let _ = t.send(ShellEvent::Workspace(WorkspaceEvent::Added {
      name: e.name,
      id: e.id,
    }));
  });

  let t = tx.clone();
  listener.add_workspace_deleted_handler(move |e| {
    let _ = t.send(ShellEvent::Workspace(WorkspaceEvent::Deleted {
      name: e.name,
      id: e.id,
    }));
  });

  listener.start_listener()?;
  Ok(())
}
