use anyhow::{Result, anyhow};
use corona_ipc::{IpcCommand, IpcServer, Reply};
use corona_launcher::{
  dmenu,
  ui::{self, Open as OpenWith},
};
use gpui_kit::App;
use serde::{Deserialize, Serialize};

pub fn register_commands(server: &mut IpcServer) {
  server
    .register::<Open>()
    .register::<Close>()
    .register::<Toggle>()
    .register::<Dmenu>();
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenRequest {
  /// Id of a command to open straight into
  pub command: Option<String>,
  /// Text the search starts with, or the command's arguments
  pub query: Option<String>,
}

pub struct Open;

impl IpcCommand for Open {
  const COMMAND: &'static str = "launcher:open";

  type Payload = OpenRequest;
  type Response = ();

  fn handle(request: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let query = request.query.unwrap_or_default();
    let open = match request.command {
      Some(id) => OpenWith::Command { id, query },
      None => OpenWith::Root { query },
    };
    ui::open(open, cx)
  }
}

pub struct Close;

impl IpcCommand for Close {
  const COMMAND: &'static str = "launcher:close";

  type Payload = ();
  type Response = ();

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    ui::close(cx);
    Ok(())
  }
}

pub struct Toggle;

impl IpcCommand for Toggle {
  const COMMAND: &'static str = "launcher:toggle";

  type Payload = ();
  type Response = ();

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    ui::toggle(cx)
  }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DmenuRequest {
  pub lines: Vec<String>,
  pub prompt: Option<String>,
  pub placeholder: Option<String>,
  pub multi: bool,
}

/// Opens the launcher on the lines and answers with the choice once the user makes one
pub struct Dmenu;

impl IpcCommand for Dmenu {
  const COMMAND: &'static str = "launcher:dmenu";

  type Payload = DmenuRequest;
  /// The chosen lines; an error when cancelled
  type Response = Vec<String>;

  fn handle(_: Self::Payload, _: &mut App) -> Result<Self::Response> {
    Err(anyhow!("launcher:dmenu answers asynchronously"))
  }

  fn reply(request: Self::Payload, cx: &mut App) -> Reply<Self::Response> {
    let (view, choice) = dmenu::view(dmenu::Options {
      lines: request.lines,
      prompt: request.prompt,
      placeholder: request.placeholder,
      multi: request.multi,
    });
    let opened = ui::open(OpenWith::View(view), cx);
    Box::pin(async move {
      opened?;
      choice.await.map_err(|_| anyhow!("cancelled"))
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn requests_json() {
    let open: OpenRequest = serde_json::from_str(r#"{"command":"volume","query":"40"}"#).unwrap();
    assert_eq!(open.command.as_deref(), Some("volume"));
    assert_eq!(open.query.as_deref(), Some("40"));
    let dmenu: DmenuRequest =
      serde_json::from_str(r#"{"lines":["a","b"],"prompt":null,"placeholder":null,"multi":true}"#)
        .unwrap();
    assert_eq!(dmenu.lines, ["a", "b"]);
    assert!(dmenu.multi);
  }
}
