use anyhow::{Context, Result, bail};
use corona_ipc::{IpcCommand, IpcServer};
use corona_pipewire::{
  PipewireExt,
  volume::{to_linear, to_slider},
};
use gpui_kit::App;
use serde::{Deserialize, Serialize};

pub fn register_commands(server: &mut IpcServer) {
  server.register::<Volume>();
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Device {
  Output,
  Input,
}

/// Levels on the volume slider's scale, 0 to 1
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Action {
  Set(f32),
  Change(f32),
  ToggleMute,
}

/// `65` and `65%` as 0.65, values up to 1 without `%` as they are
pub fn parse_level(s: &str) -> Result<f32> {
  let (number, percent) = match s.trim().strip_suffix('%') {
    Some(number) => (number, true),
    None => (s.trim(), false),
  };
  let value: f32 = number
    .trim()
    .parse()
    .context("expected a number like 65, 65% or 0.65")?;
  if !value.is_finite() || value < 0. {
    bail!("expected a positive number");
  }
  Ok(if percent || value > 1. {
    value / 100.
  } else {
    value
  })
}

pub struct Volume;

impl IpcCommand for Volume {
  const COMMAND: &'static str = "volume:change";

  type Payload = (Device, Action);
  type Response = ();

  fn handle((device, action): Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let pipewire = cx.pipewire();
    let node = match device {
      Device::Output => pipewire.default_sink(cx).context("no output device")?,
      Device::Input => pipewire.default_source(cx).context("no input device")?,
    }
    .clone();
    let audio = pipewire.audio();
    let level = match action {
      Action::Set(level) => level,
      Action::Change(delta) => to_slider(node.volume()) + delta,
      Action::ToggleMute => return audio.set_mute(node.id, !node.mute),
    };
    let volume = to_linear(level.clamp(0., 1.));
    audio.set_volumes(node.id, vec![volume; node.volumes.len().max(1)])
  }
}

#[cfg(test)]
mod tests {
  use super::parse_level;

  #[test]
  fn levels() {
    assert_eq!(parse_level("65").unwrap(), 0.65);
    assert_eq!(parse_level("65%").unwrap(), 0.65);
    assert_eq!(parse_level("0.65").unwrap(), 0.65);
    assert_eq!(parse_level("0.5%").unwrap(), 0.005);
    assert_eq!(parse_level("1").unwrap(), 1.);
    assert!(parse_level("-5").is_err());
    assert!(parse_level("loud").is_err());
  }
}
