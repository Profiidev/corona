//! The `screenshot:start`, `colorpicker:start` and `ruler:start` IPC commands

use anyhow::Result;
use corona_ipc::{IpcCommand, IpcExt};
use gpui_kit::App;

use crate::overlay::{
  colorpicker::state::ColorPickerState, ruler::state::RulerState,
  screenshot::state::ScreenshotState,
};

pub use crate::overlay::{ruler::mode::RulerMode, screenshot::mode::Mode};

pub(crate) fn register(cx: &mut App) {
  cx.ipc_mut()
    .register::<Screenshot>()
    .register::<ColorPicker>()
    .register::<Ruler>();
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

pub struct Ruler;

impl IpcCommand for Ruler {
  const COMMAND: &'static str = "ruler:start";

  type Payload = RulerMode;
  type Response = ();

  fn handle(payload: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    RulerState::start(payload, std::time::Duration::ZERO, cx);
    Ok(())
  }
}
