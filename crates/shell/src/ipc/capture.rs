//! The `screenshot:start`, `colorpicker:start` and `ruler:start` IPC commands

use anyhow::Result;
use corona_config::ConfigProvider;
use corona_ipc::{IpcCommand, IpcExt};
use gpui_kit::App;
use serde::{Deserialize, Serialize};

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

/// What to open; `live` forces the live screen, else the config decides
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ScreenshotStart {
  pub mode: Mode,
  #[serde(default)]
  pub live: bool,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct ColorPickerStart {
  #[serde(default)]
  pub live: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct RulerStart {
  pub mode: RulerMode,
  #[serde(default)]
  pub live: bool,
}

pub struct Screenshot;

impl IpcCommand for Screenshot {
  const COMMAND: &'static str = "screenshot:start";

  type Payload = ScreenshotStart;
  type Response = ();

  fn handle(payload: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let live = payload.live || cx.config().screenshot.live;
    ScreenshotState::capture(payload.mode, live, cx);
    Ok(())
  }
}

pub struct ColorPicker;

impl IpcCommand for ColorPicker {
  const COMMAND: &'static str = "colorpicker:start";

  type Payload = ColorPickerStart;
  type Response = ();

  fn handle(payload: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let live = payload.live || cx.config().color_picker.live;
    ColorPickerState::capture(live, cx);
    Ok(())
  }
}

pub struct Ruler;

impl IpcCommand for Ruler {
  const COMMAND: &'static str = "ruler:start";

  type Payload = RulerStart;
  type Response = ();

  fn handle(payload: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let live = payload.live || cx.config().ruler.live;
    RulerState::start(payload.mode, std::time::Duration::ZERO, live, cx);
    Ok(())
  }
}
