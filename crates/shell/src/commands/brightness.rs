use anyhow::{Result, bail};
use corona_brightness::{BrightnessExt, Display};
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

/// `*` picks every available display, anything else matches output name or id
fn select(displays: &[Display], name: &str) -> Vec<Display> {
  displays
    .iter()
    .filter(|d| match name {
      "*" => d.unavailable.is_none(),
      name => d.output.as_deref() == Some(name) || d.id == name,
    })
    .cloned()
    .collect()
}

/// The raw brightness value `action` sets on `d`
fn raw(action: Action, d: &Display) -> u32 {
  let level = match action {
    Action::Set(level) => level,
    Action::Change(delta) => d.percent() / 100. + delta,
  };
  (level.clamp(0., 1.) * d.max as f32).round() as u32
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
      Some(name) => select(brightness.list_displays(cx), name),
    };
    if displays.is_empty() {
      bail!(
        "no display for {}",
        monitor.as_deref().unwrap_or("the focused monitor")
      );
    }

    for d in displays {
      let task = brightness.set_brightness(&d.id, raw(action, &d), cx);
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

#[cfg(test)]
mod tests {
  use super::*;
  use corona_brightness::{DisplayKind, Unavailable};

  fn display(id: &str, output: Option<&str>, brightness: u32, max: u32) -> Display {
    Display {
      id: id.into(),
      output: output.map(Into::into),
      name: None,
      kind: DisplayKind::External,
      brightness,
      max,
      unavailable: None,
    }
  }

  fn ids(displays: Vec<Display>) -> Vec<String> {
    displays.into_iter().map(|d| d.id).collect()
  }

  #[test]
  fn select_all_skips_unavailable() {
    let mut off = display("ddc-2", Some("DP-2"), 0, 100);
    off.unavailable = Some(Unavailable::Detecting);
    let displays = [
      display("backlight", Some("eDP-1"), 1, 10),
      off,
      display("ddc-3", None, 0, 100),
    ];
    assert_eq!(ids(select(&displays, "*")), ["backlight", "ddc-3"]);
    assert!(select(&[], "*").is_empty());
  }

  #[test]
  fn select_by_output_or_id() {
    let displays = [
      display("backlight", Some("eDP-1"), 1, 10),
      display("ddc-3", None, 0, 100),
    ];
    assert_eq!(ids(select(&displays, "eDP-1")), ["backlight"]);
    assert_eq!(ids(select(&displays, "backlight")), ["backlight"]);
    assert_eq!(ids(select(&displays, "ddc-3")), ["ddc-3"]);
    assert!(select(&displays, "HDMI-A-1").is_empty());
    assert!(select(&displays, "").is_empty());
  }

  #[test]
  fn raw_set_scales_and_clamps() {
    let d = display("d", None, 0, 255);
    assert_eq!(raw(Action::Set(0.), &d), 0);
    assert_eq!(raw(Action::Set(0.5), &d), 128);
    assert_eq!(raw(Action::Set(1.), &d), 255);
    assert_eq!(raw(Action::Set(2.), &d), 255);
    assert_eq!(raw(Action::Set(-1.), &d), 0);
  }

  #[test]
  fn raw_change_is_relative() {
    let d = display("d", None, 50, 100);
    assert_eq!(raw(Action::Change(0.1), &d), 60);
    assert_eq!(raw(Action::Change(-0.2), &d), 30);
    assert_eq!(raw(Action::Change(0.9), &d), 100);
    assert_eq!(raw(Action::Change(-0.9), &d), 0);
    assert_eq!(raw(Action::Change(0.), &d), 50);
  }

  #[test]
  fn raw_zero_max() {
    let d = display("d", None, 0, 0);
    assert_eq!(raw(Action::Set(1.), &d), 0);
    assert_eq!(raw(Action::Change(0.5), &d), 0);
  }
}
