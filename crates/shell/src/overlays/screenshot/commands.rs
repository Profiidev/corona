use anyhow::Result;
use corona_ipc::{IpcCommand, IpcServer};
use gpui_kit::App;

use crate::overlays::screenshot::{mode::Mode, state::ScreenshotState};

pub fn register_commands(server: &mut IpcServer) {
  server.register::<Screenshot>();
}

pub struct Screenshot;

impl IpcCommand for Screenshot {
  const COMMAND: &'static str = "screenshot:start";

  type Payload = Mode;
  type Response = ();

  fn handle(payload: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    ScreenshotState::capture(payload, cx);
    Ok(())
  }
}
