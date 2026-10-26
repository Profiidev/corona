use std::env;

use anyhow::{Result, bail};
use wayland_client::Connection;
use wayland_protocols::ext::foreign_toplevel_list::v1::client::ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1;

use crate::hyprland::Hyprland;

/// Finds the ext-foreign-toplevel of a compositor window id, each compositor
/// has its own protocol for that.
pub(crate) trait ToplevelMapper {
  /// The index into `toplevels` of window `id`
  fn find(&mut self, toplevels: &[ExtForeignToplevelHandleV1], id: &str) -> Result<usize>;
}

pub(crate) fn mapper(conn: &Connection) -> Result<Box<dyn ToplevelMapper>> {
  if env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
    Ok(Box::new(Hyprland::new(conn)?))
  } else {
    bail!("window capture is not supported by this compositor")
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn needs_a_known_compositor() {
    // nextest runs each test in its own process, so the environment is ours
    unsafe { env::remove_var("HYPRLAND_INSTANCE_SIGNATURE") };
    // never read from: no compositor needed
    let (socket, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
    let conn = Connection::from_socket(socket).unwrap();
    let error = mapper(&conn).err().unwrap();
    assert_eq!(
      error.to_string(),
      "window capture is not supported by this compositor"
    );
  }
}
