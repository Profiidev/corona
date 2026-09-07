use std::{
  fmt::Display,
  io::{Read, Write},
  os::unix::net::UnixStream,
  path::PathBuf,
};

use anyhow::Result;

use crate::compositor::hyprland::encoding::decode_ipc_response;

#[derive(Clone)]
pub struct Ipc {
  pub cmd_socket: PathBuf,
}

impl Ipc {
  pub(super) fn send_cmd(&self, cmd: &Command) -> Result<String> {
    let mut socket = UnixStream::connect(&self.cmd_socket)?;
    socket.write_all(cmd.to_string().as_bytes())?;
    let mut res = Vec::new();
    socket.read_to_end(&mut res)?;
    Ok(decode_ipc_response(&res))
  }
}

/// https://github.com/hyprland-community/hyprland-rs/blob/master/src/data/regular.rs
pub struct Command {
  pub command: String,
  pub flags: CommandFlags,
}

bitflags::bitflags! {
  pub struct CommandFlags: u8 {
    const JSON = 1;
    const REFRESH = 2;
  }
}

impl Display for Command {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    if self.flags.contains(CommandFlags::JSON) {
      f.write_str("j")?;
    }
    if self.flags.contains(CommandFlags::REFRESH) {
      f.write_str("r")?;
    }
    write!(f, "/{}", self.command)
  }
}

#[macro_export]
macro_rules! data_cmd {
  ($cmd:ident, $arg:literal, $output:ty, $parsed:ty, $convert:expr) => {
    impl $crate::compositor::hyprland::command::Ipc {
      pub fn $cmd(&self) -> anyhow::Result<$parsed> {
        let cmd = $crate::compositor::hyprland::command::Command {
          command: $arg.to_string(),
          flags: $crate::compositor::hyprland::command::CommandFlags::JSON,
        };
        let res = self.send_cmd(&cmd)?;
        let output: $output = serde_json::from_str(&res)?;
        Ok($convert(output))
      }
    }
  };
}
