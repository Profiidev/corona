use std::time::Duration;

use crate as ci;
use crate::IdleExt;
use anyhow::{Result, ensure};
use corona_js::{Cx, ModuleBuilder, PluginRef, ScriptModule, Throws, host_module};
use gpui_kit::{App, BorrowAppContext};
use specta::specta;

fn stay_awake_for_minutes(minutes: u32, cx: &mut App) -> Result<()> {
  ensure!(minutes > 0, "expected at least one minute");
  let duration = Duration::from_secs(u64::from(minutes) * 60);
  cx.update_global::<ci::Idle, _>(|idle, cx| idle.set_stay_awake(true, Some(duration), cx));
  Ok(())
}

/// No idle behaviors and no sleep until turned off.
#[specta]
fn set_stay_awake(mut cx: Cx, on: bool) {
  cx.update_global::<ci::Idle, _>(|idle, cx| idle.set_stay_awake(on, None, cx))
}

/// Stays awake, turned off again after `minutes`.
#[specta]
fn stay_awake_for(mut cx: Cx, minutes: u32) -> Throws<()> {
  stay_awake_for_minutes(minutes, &mut cx).into()
}

/// `corona/idle`
pub struct IdleModule;

impl ScriptModule for IdleModule {
  const NAME: &str = "idle";
  const DOC: &str = "Staying awake, and whether idling is held back.";

  fn module(_: &PluginRef, m: &mut ModuleBuilder, _: &mut App) {
    m.watch(
      "stayAwake",
      "Whether the user asked to stay awake.",
      |cx| cx.idle().stay_awake.clone(),
      |cx| cx.idle().stay_awake(cx),
    );
    m.watch(
      "inhibited",
      "Staying awake, or an app holds idling back.",
      |cx| cx.idle().inhibited.clone(),
      |cx| cx.idle().inhibited(cx),
    );
    host_module!(m, { sync: [set_stay_awake, stay_awake_for] });
  }
}

#[cfg(test)]
mod tests {
  use corona_utils::test_bus::{TestBus, wait_until};
  use futures_lite::future::block_on;
  use gpui_kit::{self as gpui, TestAppContext};
  use serde_json::json;

  use super::*;
  use corona_script::harness;

  #[gpui::test]
  fn stays_awake(cx: &mut TestAppContext) {
    // the watcher finds no compositor, never the one running the tests
    let dir = tempfile::tempdir().unwrap();
    unsafe {
      std::env::set_var("XDG_RUNTIME_DIR", dir.path());
      std::env::set_var("WAYLAND_DISPLAY", "nothing-here");
    }
    cx.executor().allow_parking();
    let bus = TestBus::new();
    let session = block_on(bus.conn());
    cx.update(|cx| crate::start(cx, &session, |_, _, _| {}));

    let body = r#"if (!globalThis.started) {
      globalThis.started = true;
      report(attempt(() => m.stayAwakeFor(0)));
      m.stayAwakeFor(30);
    }
    report({ stayAwake: m.stayAwake(), inhibited: m.inhibited() });"#;
    let (view, cx) = harness::module::<IdleModule>(cx, body);
    wait_until(cx, |cx| view.reports(cx).len() >= 2);
    assert_eq!(
      view.reports(cx)[0],
      json!({ "message": "expected at least one minute" })
    );
    assert_eq!(
      view.last(cx),
      json!({ "stayAwake": true, "inhibited": true })
    );

    // turned off elsewhere, the view renders again
    cx.update(|_, cx| {
      cx.update_global::<ci::Idle, _>(|idle, cx| idle.set_stay_awake(false, None, cx))
    });
    wait_until(cx, |cx| {
      view.last(cx) == json!({ "stayAwake": false, "inhibited": false })
    });
  }
}
