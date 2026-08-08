//! Everything that can happen off the main (calloop) thread — Hyprland IPC, D-Bus calls, the
//! hot-reload file watcher — funnels through one channel into this enum so AppState only ever
//! mutates surfaces from the single-threaded event loop.

use calloop::channel::Sender;

#[derive(Debug, Clone)]
pub struct WorkspaceInfo {
  pub id: i32,
  pub name: String,
  pub active: bool,
}

#[derive(Debug, Clone)]
pub enum ShellEvent {
  Workspaces(Vec<WorkspaceInfo>),
  ActiveWindowTitle(Option<String>),
  Notify {
    id: u32,
    app_name: String,
    summary: String,
    body: String,
    timeout_ms: i32,
  },
  CloseNotification(u32),
  ShowOsd {
    label: String,
    value: f32,
  },
  ToggleCalendar,
  #[cfg(feature = "hot-reload")]
  UiChanged,
}

pub type ShellSender = Sender<ShellEvent>;
