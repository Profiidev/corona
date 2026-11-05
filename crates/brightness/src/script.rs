use crate as br;
use crate::BrightnessExt;
use corona_js::{Cx, Glob, Json, ModuleBuilder, PluginRef, ScriptModule, Throws, host_module};
use gpui_kit::App;
use serde::Serialize;
use specta::{Type, specta};

#[derive(Serialize, Type)]
#[serde(rename_all = "snake_case")]
enum DisplayKind {
  Backlight,
  External,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "snake_case")]
enum Unavailable {
  DdcutilDisabled,
  DdcutilMissing,
  Detecting,
  Unsupported,
  Failed,
}

#[derive(Serialize, Type)]
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

#[derive(Serialize, Type)]
struct Ddcutil {
  available: bool,
  enabled: bool,
}

#[derive(Serialize, Type)]
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

#[specta]
fn ddcutil(cx: Cx) -> Json<Ddcutil> {
  let brightness = cx.brightness();
  Json(Ddcutil {
    available: brightness.ddcutil_available,
    enabled: brightness.ddcutil_enabled,
  })
}

/// In a timed mode this lasts until its next start or end.
#[specta]
fn set_nightlight(cx: Cx, on: bool) -> Throws<()> {
  cx.brightness().clone().set_nightlight(on, &cx).into()
}

/// Kelvin at night, clamped to 1000 to 6500; returns what was set.
#[specta]
fn set_nightlight_temperature(cx: Cx, kelvin: u32) -> Throws<u32> {
  (cx.brightness().clone())
    .set_nightlight_temperature(kelvin, &cx)
    .into()
}

#[specta]
async fn set_brightness(
  cx: Cx,
  brightness: Glob<br::Brightness>,
  id: String,
  value: u32,
) -> Throws<()> {
  let set = brightness.set_brightness(&id, value, &cx);
  set.await.into()
}

/// `corona/brightness`
pub struct BrightnessModule;

impl ScriptModule for BrightnessModule {
  const NAME: &str = "brightness";
  const DOC: &str = "Screen brightness.";

  fn module(_: &PluginRef, m: &mut ModuleBuilder, _: &mut App) {
    m.watch(
      "listDisplays",
      "",
      |cx| cx.brightness().displays.clone(),
      |cx| {
        let displays = cx.brightness().list_displays(cx);
        Json(displays.iter().map(Display::from).collect::<Vec<_>>())
      },
    );
    m.watch(
      "nightlight",
      "",
      |cx| cx.brightness().nightlight.clone(),
      |cx| Json(Nightlight::from(cx.brightness().nightlight(cx))),
    );
    host_module!(m, {
      sync: [ddcutil, set_nightlight, set_nightlight_temperature],
      async: [set_brightness],
    });
  }
}

#[cfg(test)]
mod tests {
  use corona_config::Config;
  use corona_utils::test_bus::{TestBus, wait_until};
  use gpui_kit::{self as gpui, TestAppContext};
  use serde_json::json;

  use super::*;
  use corona_script::harness;

  #[gpui::test]
  fn reads_and_failures(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (_bus, conn) = TestBus::with_conn();
    let mut config = Config::default();
    // never the real monitors
    config.brightness.enable_ddcutil = false;
    cx.set_global(config);
    unsafe { std::env::set_var("WAYLAND_DISPLAY", "nothing-here") };
    cx.update(|cx| br::connect(cx, &conn).unwrap());

    let body = r#"if (!globalThis.started) {
      globalThis.started = true;
      m.setBrightness("nope", 5).catch((e) => report({ message: e.message }));
      report(attempt(() => m.setNightlight(true)));
    }
    report({ displays: Array.isArray(m.listDisplays()), ddcutil: m.ddcutil(), nightlight: m.nightlight() });"#;
    let (view, cx) = harness::module::<BrightnessModule>(cx, body);
    wait_until(cx, |cx| view.reports(cx).len() >= 3);

    let reports = view.reports(cx);
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
