use anyhow::{Context, Result};
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

/// The linear per-channel volumes `action` sets, none for [`Action::ToggleMute`]
fn volumes(action: Action, current: f32, channels: usize) -> Option<Vec<f32>> {
  let level = match action {
    Action::Set(level) => level,
    Action::Change(delta) => to_slider(current) + delta,
    Action::ToggleMute => return None,
  };
  Some(vec![to_linear(level.clamp(0., 1.)); channels.max(1)])
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
    match volumes(action, node.volume(), node.volumes.len()) {
      Some(volumes) => audio.set_volumes(node.id, volumes),
      None => audio.set_mute(node.id, !node.mute),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn single(action: Action, current: f32) -> f32 {
    let v = volumes(action, current, 1).unwrap();
    assert_eq!(v.len(), 1);
    v[0]
  }

  #[test]
  fn set_maps_slider_to_linear_and_clamps() {
    assert!((single(Action::Set(0.5), 0.) - 0.125).abs() < 1e-6);
    assert_eq!(single(Action::Set(1.5), 0.), 1.);
    assert_eq!(single(Action::Set(-0.5), 1.), 0.);
  }

  #[test]
  fn change_is_relative_on_slider_scale() {
    // linear 0.125 is 0.5 on the slider
    let v = single(Action::Change(0.25), 0.125);
    assert!((v - to_linear(0.75)).abs() < 1e-5, "{v}");
    assert_eq!(single(Action::Change(1.), 0.9), 1.);
    assert_eq!(single(Action::Change(-1.), 0.1), 0.);
    assert!((single(Action::Change(0.), 0.3) - 0.3).abs() < 1e-5);
  }

  #[test]
  fn every_channel_gets_the_same_volume() {
    assert_eq!(volumes(Action::Set(1.), 0., 0).unwrap(), [1.]);
    assert_eq!(volumes(Action::Set(1.), 0., 2).unwrap(), [1., 1.]);
    assert_eq!(volumes(Action::Set(0.), 0., 6).unwrap().len(), 6);
  }

  #[test]
  fn toggle_mute_sets_no_volume() {
    assert!(volumes(Action::ToggleMute, 0.5, 2).is_none());
  }
}
