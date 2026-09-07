use std::{
  io::{Read, Write},
  os::unix::net::UnixStream,
  path::{Path, PathBuf},
};

use anyhow::Result;
use gpui_kit::App;

use crate::compositor::{
  Compositor,
  hyprland::{command::Command, encoding::decode_ipc_response},
  types,
};

mod command;
mod encoding;
mod workspace;

pub struct Hyprland {
  cmd_socket: PathBuf,
}

impl Hyprland {
  pub fn init(cx: &mut App, socket_dir: &Path) -> Self {
    let cmd_socket = socket_dir.join(".socket.sock");
    let event_path = socket_dir.join(".socket2.sock");

    Hyprland { cmd_socket }
  }

  fn send_cmd(&self, cmd: &Command) -> Result<String> {
    let mut socket = UnixStream::connect(&self.cmd_socket)?;
    socket.write_all(cmd.to_string().as_bytes())?;
    let mut res = Vec::new();
    socket.read_to_end(&mut res)?;
    Ok(decode_ipc_response(&res))
  }
}

impl Compositor for Hyprland {
  fn list_workspaces(&self) -> Result<Vec<types::Workspace>> {
    let workspaces = self.get_workspaces()?;
    let workspaces = workspaces.into_iter().map(Into::into).collect();
    Ok(workspaces)
  }
}
