use anyhow::{Result, bail};
use corona_brightness::BrightnessExt;
use corona_ipc::{IpcCommand, IpcServer};
use corona_utils::error::ErrorLogExt;
use gpui_kit::App;
use serde::{Deserialize, Serialize};

use crate::control_center::dashboard::sliders::display;

pub fn register_commands(server: &mut IpcServer) {
  server
    .register::<SetBrightness>()
    .register::<ListMonitors>();
}

/// Levels from 0 to 1
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Action {
  Set(f32),
  Change(f32),
}

pub struct SetBrightness;

impl IpcCommand for SetBrightness {
  const COMMAND: &'static str = "brightness:set";

  /// the monitor: its output name, `*` for all, none for the focused one
  type Payload = (Option<String>, Action);
  type Response = ();

  fn handle((monitor, action): Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let brightness = cx.brightness().clone();
    let displays: Vec<_> = match monitor.as_deref() {
      None => display(cx).into_iter().cloned().collect(),
      Some("*") => brightness
        .list_displays(cx)
        .iter()
        .filter(|d| d.unavailable.is_none())
        .cloned()
        .collect(),
      Some(name) => brightness
        .list_displays(cx)
        .iter()
        .filter(|d| d.output.as_deref() == Some(name) || d.id == name)
        .cloned()
        .collect(),
    };
    if displays.is_empty() {
      bail!(
        "no display for {}",
        monitor.as_deref().unwrap_or("the focused monitor")
      );
    }

    for d in displays {
      let level = match action {
        Action::Set(level) => level,
        Action::Change(delta) => d.percent() / 100. + delta,
      };
      let raw = (level.clamp(0., 1.) * d.max as f32).round() as u32;
      let task = brightness.set_brightness(&d.id, raw, cx);
      cx.spawn(async move |_| {
        let _ = task.await.log_err();
      })
      .detach();
    }
    Ok(())
  }
}

pub struct ListMonitors;

impl IpcCommand for ListMonitors {
  const COMMAND: &'static str = "brightness:list";

  type Payload = ();
  type Response = Vec<String>;

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    Ok(
      cx.brightness()
        .list_displays(cx)
        .iter()
        .map(|d| d.output.clone().unwrap_or_else(|| d.id.clone()))
        .collect(),
    )
  }
}
