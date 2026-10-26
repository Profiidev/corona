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
  /// `None` when the display does not tell its model
  name: Option<String>,
  kind: DisplayKind,
  brightness: u32,
  max: u32,
  unavailable: Option<Unavailable>,
}

impl From<&br::Display> for Display {
  fn from(d: &br::Display) -> Self {
    Self {
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
    }
  }
}

#[derive(Serialize, TS)]
struct Ddcutil {
  available: bool,
  enabled: bool,
}

#[derive(Serialize, TS)]
struct Nightlight {
  /// The compositor lets the shell change gamma
  available: bool,
  on: bool,
  /// Kelvin at night
  temperature: u32,
}

impl From<&br::Nightlight> for Nightlight {
  fn from(n: &br::Nightlight) -> Self {
    Self {
      available: n.available,
      on: n.on,
      temperature: n.temperature,
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Updates {
  Displays,
  Nightlight,
}

impl From<Updates> for super::Updates {
  fn from(value: Updates) -> Self {
    super::Updates::Brightness(value)
  }
}

pub fn module(reads: &Subscriptions, subs: &mut Vec<Subscribe>) -> HostModule {
  Module::new("corona/brightness")
    .func(read(
      reads,
      subs,
      "listDisplays",
      Updates::Displays,
      |cx| cx.brightness().displays.clone(),
      |cx| {
        let displays = cx.brightness().list_displays(cx);
        displays.iter().map(Display::from).collect::<Vec<_>>()
      },
    ))
    .func(named!("ddcutil", |cx: Cx| {
      let brightness = cx.brightness();
      Ddcutil {
        available: brightness.ddcutil_available,
        enabled: brightness.ddcutil_enabled,
      }
    }))
    .func(read(
      reads,
      subs,
      "nightlight",
      Updates::Nightlight,
      |cx| cx.brightness().nightlight.clone(),
      |cx| Nightlight::from(cx.brightness().nightlight(cx)),
    ))
    .func(named!(
      "setNightlight",
      /// In a timed mode this lasts until its next start or end.
      |cx: &mut App, on: bool| cx.brightness().clone().set_nightlight(on, cx)
    ))
    .func(named!(
      "setNightlightTemperature",
      /// Kelvin at night, clamped to 1000 to 6500; returns what was set.
      |cx: &mut App, kelvin: u32| cx.brightness().clone().set_nightlight_temperature(kelvin, cx)
    ))
    .func(named!(
      "setBrightness",
      |cx: Cx, brightness: Glob<Brightness>, id: String, value: u32| {
        brightness.set_brightness(&id, value, &cx)
      }
    ))
    .into()
}

#[cfg(test)]
mod tests {
  use corona_config::Config;
  use corona_utils::test_bus::{TestBus, wait_until};
  use futures_lite::future::block_on;
  use gpui_kit::{self as gpui, TestAppContext};
  use serde_json::json;

  use super::*;
  use crate::module::harness;

  #[gpui::test]
  fn reads_and_failures(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let bus = TestBus::new();
    let conn = block_on(bus.conn());
    let mut config = Config::default();
    // never the real monitors
    config.brightness.enable_ddcutil = false;
    cx.set_global(config);
    unsafe { std::env::set_var("WAYLAND_DISPLAY", "nothing-here") };
    cx.update(|cx| br::init(cx, &conn).unwrap());

    let body = r#"if (!globalThis.started) {
      globalThis.started = true;
      m.setBrightness("nope", 5).then(report);
      report(m.setNightlight(true));
    }
    report({ displays: Array.isArray(m.listDisplays()), ddcutil: m.ddcutil(), nightlight: m.nightlight() });"#;
    let (view, cx) = harness::view(cx, body, |reads, subs, _| module(reads, subs));
    wait_until(cx, |_| view.reports.borrow().len() >= 3);

    let reports = view.reports.borrow();
    assert!(
      reports.contains(&json!({ "message": "no display nope" })),
      "{reports:?}"
    );
    let read = reports.iter().find(|r| r.get("ddcutil").is_some()).unwrap();
    assert_eq!(read["displays"], true);
    assert_eq!(read["ddcutil"]["enabled"], false);
    assert!(read["ddcutil"]["available"].is_boolean());
    // no compositor in the tests: no gamma either
    assert_eq!(read["nightlight"]["available"], false);
    assert_eq!(read["nightlight"]["temperature"], 4000);
    assert!(
      reports.iter().any(|r| r["message"]
        .as_str()
        .is_some_and(|e| e.contains("nightlight is unavailable"))),
      "{reports:?}"
    );
    // listing renders again as displays come and go, `ddcutil` is fixed
    assert!(view.reads.contains(Updates::Displays.into()));
    assert!(view.reads.contains(Updates::Nightlight.into()));
  }

  fn display(kind: br::DisplayKind, unavailable: Option<br::Unavailable>) -> serde_json::Value {
    let display = br::Display {
      id: "ddc:1".into(),
      output: Some("DP-1".into()),
      name: None,
      kind,
      brightness: 40,
      max: 100,
      unavailable,
    };
    serde_json::to_value(Display::from(&display)).unwrap()
  }

  #[test]
  fn displays() {
    let json = display(br::DisplayKind::Backlight, None);
    assert_eq!(json["kind"], "backlight");
    assert_eq!(json["id"], "ddc:1");
    assert_eq!(json["output"], "DP-1");
    assert!(json["name"].is_null());
    assert_eq!(json["brightness"], 40);
    assert_eq!(json["max"], 100);
    assert!(json["unavailable"].is_null());

    let all = [
      (br::Unavailable::DdcutilDisabled, "ddcutil_disabled"),
      (br::Unavailable::DdcutilMissing, "ddcutil_missing"),
      (br::Unavailable::Detecting, "detecting"),
      (br::Unavailable::Unsupported, "unsupported"),
      (br::Unavailable::Failed, "failed"),
    ];
    for (reason, name) in all {
      let json = display(br::DisplayKind::External, Some(reason));
      assert_eq!(json["kind"], "external");
      assert_eq!(json["unavailable"], name);
    }
  }
}
