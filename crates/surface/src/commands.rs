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

#[cfg(test)]
mod tests {
  use gpui_kit::TestAppContext;

  use super::*;
  use crate::{
    panel::AppPanelExt,
    test_support::{self, PanelA, PanelB},
  };

  #[test]
  fn command_names_are_stable() {
    // clients of other versions send these
    assert_eq!(TogglePanel::COMMAND, "surface:toggle_panel");
    assert_eq!(OpenPanel::COMMAND, "surface:open_panel");
    assert_eq!(ClosePanel::COMMAND, "surface:close_panel");
    assert_eq!(ListPanels::COMMAND, "surface:list_panels");
  }

  #[gpui_kit::test]
  fn handlers_reach_the_panels(cx: &mut TestAppContext) {
    test_support::setup(cx);
    cx.update(|cx| {
      cx.panel().register::<PanelB>().register::<PanelA>();
      assert_eq!(ListPanels::handle((), cx).unwrap(), ["panel_a", "panel_b"]);
      assert!(TogglePanel::handle("nope".into(), cx).is_err());
      assert!(OpenPanel::handle("nope".into(), cx).is_err());
      // known, but no bar to open it on
      assert!(OpenPanel::handle("panel_a".into(), cx).is_err());
      assert!(ClosePanel::handle("nope".into(), cx).is_ok());
      assert!(ClosePanel::handle("panel_a".into(), cx).is_ok());
    });
  }

  #[test]
  fn registers_on_a_server() {
    let dir = tempfile::tempdir().unwrap();
    unsafe {
      std::env::set_var("XDG_RUNTIME_DIR", dir.path());
      std::env::remove_var("WAYLAND_DISPLAY");
    }
    let mut server = IpcServer::new().unwrap().expect("socket is free");
    register_commands(&mut server);
  }
}
