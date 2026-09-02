pub use hyprland::shared::{WorkspaceId, WorkspaceType};

/// Channel the background listeners (Hyprland, D-Bus) push into. Drained by
/// [`crate::Shell`] on GPUI's foreground executor.
pub type ShellSender = smol::channel::Sender<ShellEvent>;

#[derive(Clone, Debug)]
pub enum ShellEvent {
  /// Fires on every wall-clock second boundary.
  Tick,
  Notification(NotificationEvent),
  Workspace(WorkspaceEvent),
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
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
