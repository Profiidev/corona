use std::path::Path;

use anyhow::{Context, Result};
use gpui_kit::App;

use crate::{CompositorImpl, hyprland::command::Ipc, types};

mod command;
mod cursor;
mod encoding;
mod event;
mod keyboard;
mod monitor;
mod windows;
mod workspace;

pub struct Hyprland {
  ipc: Ipc,
}

impl Hyprland {
  pub fn init(cx: &mut App, socket_dir: &Path) -> Self {
    let cmd_socket = socket_dir.join(".socket.sock");
    let ipc = Ipc { cmd_socket };

    let event_path = socket_dir.join(".socket2.sock");
    Self::spawn_event_listener(cx, ipc.clone(), event_path);

    Hyprland { ipc }
  }
}

impl CompositorImpl for Hyprland {
  fn list_workspaces(&self) -> Result<Vec<types::Workspace>> {
    self.ipc.list_workspaces()
  }
  fn active_workspace(&self) -> Result<types::Workspace> {
    self.ipc.active_workspace()
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

  fn focus_workspace(&self, workspace: &str) -> Result<()> {
    self.ipc.focus_workspace(workspace)
  }

  fn focus_window(&self, address: &str) -> Result<()> {
    self.ipc.focus_window(address)
  }

  fn close_window(&self, address: &str) -> Result<()> {
    self.ipc.close_window(address)
  }

  fn cursor_position(&self) -> Result<(i32, i32)> {
    self.ipc.cursor_position()
  }

  fn set_dpms(&self, on: bool) -> Result<()> {
    self.ipc.dpms(if on { "on" } else { "off" })
  }

  fn keyboard_layout(&self) -> Result<Option<String>> {
    self.ipc.keyboard_layout()
  }
}
