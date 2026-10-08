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

  pub fn register<C: IpcCommand>(&mut self) -> &mut Self {
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

#[cfg(test)]
mod tests {
  use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net,
    thread,
  };

  use corona_utils::test_bus::wait_until;
  use gpui_kit::{self as gpui, TestAppContext};
  use serde_json::{Value, json};
  use tempfile::TempDir;

  use super::*;
  use crate::{IpcCommandSend, client, command::tests::Shout};

  fn runtime_dir() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    unsafe {
      std::env::set_var("XDG_RUNTIME_DIR", dir.path());
      std::env::remove_var("WAYLAND_DISPLAY");
    }
    dir
  }

  /// A running server with `Shout` registered
  fn serve(cx: &mut TestAppContext) -> TempDir {
    cx.executor().allow_parking();
    let dir = runtime_dir();
    let mut server = IpcServer::new().unwrap().unwrap();
    server.register::<Shout>();
    cx.spawn(async move |cx| server.run(&cx).await).detach();
    dir
  }

  /// Runs a blocking client on its own thread while the app keeps serving
  fn call<T: Send + 'static>(cx: &mut TestAppContext, f: impl FnOnce() -> T + Send + 'static) -> T {
    let handle = thread::spawn(f);
    wait_until(cx, |_| handle.is_finished());
    handle.join().unwrap()
  }

  /// Sends raw bytes, returns the raw reply line ("" when the server hung up)
  fn raw(cx: &mut TestAppContext, request: &'static [u8]) -> String {
    call(cx, move || {
      let mut stream = net::UnixStream::connect(util::socket_path()).unwrap();
      stream.write_all(request).unwrap();
      let mut line = String::new();
      BufReader::new(stream).read_line(&mut line).unwrap();
      line
    })
  }

  #[test]
  fn binds_a_fresh_socket() {
    let _dir = runtime_dir();
    assert!(IpcServer::new().unwrap().is_some());
    assert!(util::socket_path().exists());
  }

  #[test]
  fn yields_to_a_live_instance() {
    let _dir = runtime_dir();
    let _live = net::UnixListener::bind(util::socket_path()).unwrap();
    assert!(IpcServer::new().unwrap().is_none());
  }

  #[test]
  fn replaces_a_stale_socket() {
    let _dir = runtime_dir();
    // the file outlives the listener, nobody answers on it
    drop(net::UnixListener::bind(util::socket_path()).unwrap());
    assert!(util::socket_path().exists());
    assert!(IpcServer::new().unwrap().is_some());
  }

  #[test]
  fn missing_runtime_dir_is_an_error() {
    let dir = runtime_dir();
    unsafe { std::env::set_var("XDG_RUNTIME_DIR", dir.path().join("missing")) };
    assert!(IpcServer::new().is_err());
  }

  #[gpui::test]
  fn round_trips(cx: &mut TestAppContext) {
    let _dir = serve(cx);
    assert_eq!(call(cx, || Shout::send("hi".into()).unwrap()), "HI");
    let err = call(cx, || Shout::send("fail".into()).unwrap_err().to_string());
    assert!(err.contains("nope"));
    let err = call(cx, || {
      client::send("nope", Value::Null).unwrap_err().to_string()
    });
    assert!(err.contains("Unknown command"));
    // payload of the wrong type
    assert!(call(cx, || client::send(Shout::COMMAND, json!(5)).is_err()));
  }

  #[gpui::test]
  fn wire_format(cx: &mut TestAppContext) {
    let _dir = serve(cx);
    let ok: Value =
      serde_json::from_str(&raw(cx, b"{\"command\":\"shout\",\"data\":\"a\"}\n")).unwrap();
    assert_eq!(ok, json!({ "Ok": "A" }));
    let err: Value =
      serde_json::from_str(&raw(cx, b"{\"command\":\"x\",\"data\":null}\n")).unwrap();
    assert!(err["Err"].is_string());
  }

  #[gpui::test]
  fn survives_bad_clients(cx: &mut TestAppContext) {
    let _dir = serve(cx);
    // liveness probe: connect and hang up without a request
    call(cx, || {
      drop(net::UnixStream::connect(util::socket_path()).unwrap())
    });
    // invalid JSON: the client is hung up on instead of left waiting
    assert_eq!(raw(cx, b"not json\n"), "");
    assert!(call(cx, || client::send("x", Value::Null)).is_err());
    assert_eq!(call(cx, || Shout::send("still".into()).unwrap()), "STILL");
  }
}
