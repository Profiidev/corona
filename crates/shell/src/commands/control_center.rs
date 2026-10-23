use anyhow::{Result, bail};
use corona_ipc::{IpcCommand, IpcServer};
use corona_surface::panel::PanelState;
use gpui_kit::App;

use crate::control_center::variants::ControlCenterType;

pub fn register_commands(server: &mut IpcServer) {
  server.register::<OpenPage>().register::<ListPages>();
}

/// The control center pages by name, like `audio`
pub fn pages() -> Vec<&'static str> {
  ControlCenterType::iter().map(|p| p.as_str()).collect()
}

/// Opens the control center on a page
pub struct OpenPage;

impl IpcCommand for OpenPage {
  const COMMAND: &'static str = "control_center:open";

  type Payload = String;
  type Response = ();

  fn handle(page: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    if !pages().contains(&page.as_str()) {
      bail!(
        "unknown page {page}, expected one of {}",
        pages().join(", ")
      );
    }
    PanelState::show(&page, false, cx)
  }
}

pub struct ListPages;

impl IpcCommand for ListPages {
  const COMMAND: &'static str = "control_center:list";

  type Payload = ();
  type Response = Vec<String>;

  fn handle(_: Self::Payload, _: &mut App) -> Result<Self::Response> {
    Ok(pages().into_iter().map(String::from).collect())
  }
}
