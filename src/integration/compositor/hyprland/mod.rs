use std::path::Path;

use anyhow::{Context, Result};
use gpui_kit::{App, Entity};

use crate::integration::compositor::{
  Compositor, event::CompositorEventEmitter, hyprland::command::Ipc, types,
};

mod command;
mod encoding;
mod event;
mod monitor;
mod windows;
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
    self.ipc.list_workspaces()
  }
  fn active_workspace(&self) -> Result<types::Workspace> {
    self.ipc.active_workspace()
  }
  fn focus_workspace(&self, workspace: u32) -> Result<()> {
    self.ipc.focus_workspace(workspace)
  }

  fn list_monitors(&self) -> Result<Vec<types::Monitor>> {
    self.ipc.list_monitors()
  }
  fn active_monitor(&self) -> Result<types::Monitor> {
    self
      .ipc
      .list_monitors()?
      .into_iter()
      .find(|m| m.focused && !m.disabled)
      .context("No active monitor found")
  }

  fn list_windows(&self) -> Result<Vec<types::Window>> {
    self.ipc.list_windows()
  }
  fn active_window(&self) -> Result<Option<types::Window>> {
    self.ipc.active_window()
  }
}
