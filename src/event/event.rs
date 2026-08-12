use wayland_client::protocol::wl_output::WlOutput;

pub use hyprland::shared::{WorkspaceId, WorkspaceType};

#[derive(Clone)]
pub enum ShellEvent {
  Tick,
  Notification(NotificationEvent),
  Output(OutputEvent),
  Workspace(WorkspaceEvent),
}

#[derive(Clone)]
pub enum NotificationEvent {
  New {
    id: u32,
    app_name: String,
    app_icon: String,
    summary: String,
    body: String,
    actions: Vec<String>,
    timeout_ms: i32,
  },
  Close(u32),
}

#[derive(Clone)]
pub enum OutputEvent {
  New(WlOutput),
  Update(WlOutput),
  Destroy(WlOutput),
}

#[derive(Clone)]
pub enum WorkspaceEvent {
  Added {
    name: WorkspaceType,
    id: WorkspaceId,
  },
  Deleted {
    name: WorkspaceType,
    id: WorkspaceId,
  },
  Changed {
    name: WorkspaceType,
    id: WorkspaceId,
  },
}
