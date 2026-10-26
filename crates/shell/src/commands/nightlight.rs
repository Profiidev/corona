use anyhow::Result;
use corona_brightness::BrightnessExt;
use corona_ipc::{IpcCommand, IpcServer};
use gpui_kit::App;
use serde::{Deserialize, Serialize};

use crate::commands::radio::Switch;

pub fn register_commands(server: &mut IpcServer) {
  server.register::<Nightlight>();
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Request {
  pub switch: Option<Switch>,
  /// Kelvin at night, clamped to 1000 to 6500
  pub temperature: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
  pub on: bool,
  pub temperature: u32,
}

/// Warmer colors: on, off, toggle and the temperature. In a timed mode switching lasts until
/// its next start or end.
pub struct Nightlight;

impl IpcCommand for Nightlight {
  const COMMAND: &'static str = "nightlight:switch";

  type Payload = Request;
  /// The state afterwards
  type Response = Status;

  fn handle(request: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let brightness = cx.brightness().clone();
    let current = *brightness.nightlight(cx);
    let mut status = Status {
      on: current.on,
      temperature: current.temperature,
    };
    if let Some(kelvin) = request.temperature {
      status.temperature = brightness.set_nightlight_temperature(kelvin, cx)?;
    }
    if let Some(on) = request.switch.and_then(|s| s.target(current.on)) {
      brightness.set_nightlight(on, cx)?;
      status.on = on;
    }
    Ok(status)
  }
}
