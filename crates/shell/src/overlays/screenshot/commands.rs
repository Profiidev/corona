use anyhow::Result;
use corona_ipc::{IpcCommand, IpcExt};
use gpui_kit::App;

use crate::overlays::screenshot::{mode::Mode, state::ScreenshotState};

pub fn register_commands(cx: &mut App) {
  cx.ipc_mut().register::<Screenshot>();
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
