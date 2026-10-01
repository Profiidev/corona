use corona_brightness as br;
use corona_brightness::{Brightness, BrightnessExt};
use gpui_kit::App;
use gpui_shell::HostModule;
use serde::Serialize;
use ts_rs::TS;

use crate::{
  host_fn::{Cx, Glob, Module},
  module::{Subscribe, Subscriptions, read},
};
use corona_macros::named;

#[derive(Serialize, TS)]
#[serde(rename_all = "snake_case")]
enum DisplayKind {
  Backlight,
  External,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "snake_case")]
enum Unavailable {
  DdcutilDisabled,
  DdcutilMissing,
  Detecting,
  Unsupported,
  Failed,
}

#[derive(Serialize, TS)]
struct Display {
  id: String,
  output: Option<String>,
  name: String,
  kind: DisplayKind,
  brightness: u32,
  max: u32,
  unavailable: Option<Unavailable>,
}

#[derive(Serialize, TS)]
struct Ddcutil {
  available: bool,
  enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Updates {
  Displays,
}

impl From<Updates> for super::Updates {
  fn from(value: Updates) -> Self {
    super::Updates::Brightness(value)
  }
}

pub fn module(reads: &Subscriptions, subs: &mut Vec<Subscribe>, cx: &mut App) -> HostModule {
  let state = cx.brightness();

  Module::new("corona/brightness")
    .func(read(
      reads,
      subs,
      "listDisplays",
      Updates::Displays,
      state.displays.clone(),
      |cx| {
        let displays = cx.brightness().list_displays(cx);
        displays
          .iter()
          .map(|d| Display {
            id: d.id.clone(),
            output: d.output.clone(),
            name: d.name.clone(),
            kind: match d.kind {
              br::DisplayKind::Backlight => DisplayKind::Backlight,
              br::DisplayKind::External => DisplayKind::External,
            },
            brightness: d.brightness,
            max: d.max,
            unavailable: d.unavailable.map(|reason| match reason {
              br::Unavailable::DdcutilDisabled => Unavailable::DdcutilDisabled,
              br::Unavailable::DdcutilMissing => Unavailable::DdcutilMissing,
              br::Unavailable::Detecting => Unavailable::Detecting,
              br::Unavailable::Unsupported => Unavailable::Unsupported,
              br::Unavailable::Failed => Unavailable::Failed,
            }),
          })
          .collect::<Vec<_>>()
      },
    ))
    .func(named!("ddcutil", |cx: Cx| {
      let brightness = cx.brightness();
      Ddcutil {
        available: brightness.ddcutil_available,
        enabled: brightness.ddcutil_enabled,
      }
    }))
    .func(named!(
      "setBrightness",
      |cx: Cx, brightness: Glob<Brightness>, id: String, value: u32| {
        brightness.set_brightness(&id, value, &cx)
      }
    ))
    .into()
}
