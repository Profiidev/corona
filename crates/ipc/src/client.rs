use std::{
  io::{BufRead, BufReader, Write},
  os::unix::net::UnixStream,
};

use anyhow::{Result, anyhow};
use serde_json::Value;

use crate::{command::IpcPayload, util};

pub(crate) fn send(command: &str, data: Value) -> Result<Value> {
  let mut stream = UnixStream::connect(util::socket_path())?;

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
