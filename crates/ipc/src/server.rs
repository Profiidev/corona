use std::collections::HashMap;

use anyhow::{Result, anyhow};
use gpui_kit::AsyncApp;
use smol::{
  io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
  net::unix::{UnixListener, UnixStream},
};
use tracing::error;

use crate::{
  command::{Handler, IpcCommand, IpcPayload, erase},
  util,
};

pub struct IpcServer {
  listener: UnixListener,
  handlers: HashMap<&'static str, Handler>,
}

impl IpcServer {
  pub fn new() -> Result<Option<Self>> {
    let socket_path = util::socket_path();

    let listener = match UnixListener::bind(&socket_path) {
      Ok(listener) => listener,
      Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
        if std::os::unix::net::UnixStream::connect(&socket_path).is_ok() {
          return Ok(None);
        }
        std::fs::remove_file(&socket_path)?;
        UnixListener::bind(&socket_path)?
      }
      Err(e) => return Err(e.into()),
    };

    Ok(Some(Self {
      listener,
      handlers: HashMap::new(),
    }))
  }

  pub fn register<C: IpcCommand>(mut self) -> Self {
    self.handlers.insert(C::COMMAND, erase::<C>());
    self
  }

  async fn accept(&self) -> Result<(IpcPayload, UnixStream)> {
    loop {
      let (stream, _) = self.listener.accept().await?;
      let mut line = String::new();
      // empty connection = liveness probe from a second instance
      if BufReader::new(stream.clone()).read_line(&mut line).await? == 0 {
        continue;
      }
      return Ok((serde_json::from_str(&line)?, stream));
    }
  }

  async fn reply(mut stream: UnixStream, res: &impl serde::Serialize) -> Result<()> {
    let mut buf = serde_json::to_vec(res)?;
    buf.push(b'\n');
    Ok(stream.write_all(&buf).await?)
  }

  pub async fn run(self, cx: &AsyncApp) {
    loop {
      let (payload, stream) = match self.accept().await {
        Ok((payload, stream)) => (payload, stream),
        Err(e) => {
          error!("Error accepting connection: {}", e);
          continue;
        }
      };

      let res = match self.handlers.get(payload.command.as_str()) {
        Some(handler) => cx.update(|cx| handler(payload.data, cx)),
        None => Err(anyhow!("Unknown command: {}", payload.command)),
      }
      .map_err(|e| e.to_string());

      if let Err(e) = Self::reply(stream, &res).await {
        error!("Error replying to connection: {}", e);
      }
    }
  }
}
