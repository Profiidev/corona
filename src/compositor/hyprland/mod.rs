use std::path::Path;

use anyhow::Result;
use gpui_kit::{App, Entity};

use crate::compositor::{Compositor, event::CompositorEventEmitter, hyprland::command::Ipc, types};

mod command;
mod encoding;
mod event;
mod workspace;

pub struct Hyprland {
  ipc: Ipc,
  events: Entity<CompositorEventEmitter>,
}

impl Hyprland {
  pub fn init(cx: &mut App, socket_dir: &Path) -> Self {
    let cmd_socket = socket_dir.join(".socket.sock");
    let ipc = Ipc { cmd_socket };

    let event_path = socket_dir.join(".socket2.sock");
    let events = Self::spawn_event_listener(cx, ipc.clone(), event_path);

    Hyprland { ipc, events }
  }
}

impl Compositor for Hyprland {
  fn emitter(&self) -> &Entity<CompositorEventEmitter> {
    &self.events
  }

  fn list_workspaces(&self) -> Result<Vec<types::Workspace>> {
    self.ipc.get_workspaces()
  }
}
