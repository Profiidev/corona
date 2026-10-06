use anyhow::Result;
use corona_ipc::{IpcCommand, IpcServer};
use gpui_kit::App;

use crate::panel::PanelState;

pub fn register_commands(server: &mut IpcServer) {
  server
    .register::<TogglePanel>()
    .register::<OpenPanel>()
    .register::<ClosePanel>()
    .register::<ListPanels>();
}

pub struct TogglePanel;

impl IpcCommand for TogglePanel {
  const COMMAND: &'static str = "surface:toggle_panel";

  type Payload = String;
  type Response = ();

  fn handle(payload: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    PanelState::show(&payload, true, cx)
  }
}

pub struct OpenPanel;

impl IpcCommand for OpenPanel {
  const COMMAND: &'static str = "surface:open_panel";

  type Payload = String;
  type Response = ();

  fn handle(payload: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    PanelState::show(&payload, false, cx)
  }
}

pub struct ClosePanel;

impl IpcCommand for ClosePanel {
  const COMMAND: &'static str = "surface:close_panel";

  type Payload = String;
  type Response = ();

  fn handle(payload: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    PanelState::close(&payload, cx)
  }
}

pub struct ListPanels;

impl IpcCommand for ListPanels {
  const COMMAND: &'static str = "surface:list_panels";

  type Payload = ();
  type Response = Vec<String>;

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    Ok(PanelState::names(cx))
  }
}
