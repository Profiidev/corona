use anyhow::{Context, Result};
use corona_clipboard::{ClipboardExt, ClipboardHistory};
use corona_ipc::{IpcCommand, IpcServer};
use gpui_kit::App;
use serde::{Deserialize, Serialize};

pub fn register_commands(server: &mut IpcServer) {
  server
    .register::<ListClipboard>()
    .register::<DeleteClip>()
    .register::<ClearClipboard>();
}

fn history(cx: &App) -> Result<ClipboardHistory> {
  cx.clipboard_history()
    .cloned()
    .context("the clipboard history is off")
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClipInfo {
  pub id: i64,
  pub kind: String,
  /// The text, or the image file
  pub content: String,
  pub pinned: bool,
}

/// Newest first, searched when a query is given
pub struct ListClipboard;

impl IpcCommand for ListClipboard {
  const COMMAND: &'static str = "clipboard:list";

  type Payload = (Option<String>, usize);
  type Response = Vec<ClipInfo>;

  fn handle((query, limit): Self::Payload, cx: &mut App) -> Result<Self::Response> {
    Ok(
      history(cx)?
        .list(query.as_deref().unwrap_or(""), None, limit)
        .into_iter()
        .map(|e| ClipInfo {
          id: e.id,
          kind: format!("{:?}", e.kind).to_lowercase(),
          content: e
            .text
            .or_else(|| e.path.map(|p| p.display().to_string()))
            .unwrap_or_default(),
          pinned: e.pinned,
        })
        .collect(),
    )
  }
}

pub struct DeleteClip;

impl IpcCommand for DeleteClip {
  const COMMAND: &'static str = "clipboard:delete";

  type Payload = i64;
  type Response = ();

  fn handle(id: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    history(cx)?.delete(id, cx)
  }
}

/// Removes everything but the pinned entries
pub struct ClearClipboard;

impl IpcCommand for ClearClipboard {
  const COMMAND: &'static str = "clipboard:clear";

  type Payload = ();
  /// How many went
  type Response = usize;

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    history(cx)?.clear(cx)
  }
}
