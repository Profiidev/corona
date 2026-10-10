use std::{
  io::{BufRead, BufReader, Write},
  os::unix::net::UnixStream,
};

use anyhow::{Result, anyhow};
use serde_json::Value;

use crate::{command::IpcPayload, util};

pub(crate) fn send(command: &str, data: Value) -> Result<Value> {
  let socket = util::socket_path();
  util::private_dir(&socket)?;
  let mut stream = UnixStream::connect(socket)?;

  let payload = IpcPayload {
    command: command.to_string(),
    data,
  };
  let mut buf = serde_json::to_vec(&payload)?;
  buf.push(b'\n');
  stream.write_all(&buf)?;

  let mut line = String::new();
  BufReader::new(stream).read_line(&mut line)?;
  let res: Result<Value, String> = serde_json::from_str(&line)?;
  res.map_err(|e| anyhow!(e))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn no_server_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    unsafe {
      std::env::set_var("XDG_RUNTIME_DIR", dir.path());
      std::env::remove_var("WAYLAND_DISPLAY");
      std::env::remove_var("CORONA_SOCKET");
    }
    assert!(send("anything", Value::Null).is_err());
  }
}
