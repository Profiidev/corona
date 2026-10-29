//! The `screenshot:start` and `colorpicker:start` IPC commands

use anyhow::Result;
use corona_ipc::{IpcCommand, IpcExt};
use gpui_kit::App;

use crate::ui::{
  colorpicker::state::ColorPickerState,
  screenshot::{mode::Mode, state::ScreenshotState},
};

pub(crate) fn register(cx: &mut App) {
  cx.ipc_mut()
    .register::<Screenshot>()
    .register::<ColorPicker>();
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

pub struct ColorPicker;

impl IpcCommand for ColorPicker {
  const COMMAND: &'static str = "colorpicker:start";

  type Payload = ();
  type Response = ();

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    ColorPickerState::capture(cx);
    Ok(())
  }
}
