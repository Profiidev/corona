use anyhow::Result;
use corona_ipc::{IpcCommand, IpcExt};
use gpui_kit::App;

use crate::overlays::colorpicker::state::ColorPickerState;

pub fn register_commands(cx: &mut App) {
  cx.ipc_mut().register::<ColorPicker>();
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
