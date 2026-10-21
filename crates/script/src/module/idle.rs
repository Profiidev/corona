use std::time::Duration;

use anyhow::{Result, ensure};
use corona_idle::{Idle, IdleExt};
use gpui_kit::{App, BorrowAppContext};
use gpui_shell::HostModule;

use crate::{
  host_fn::Module,
  module::{Subscribe, Subscriptions, read},
};
use corona_macros::named;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Updates {
  StayAwake,
  Inhibited,
}

impl From<Updates> for super::Updates {
  fn from(value: Updates) -> Self {
    super::Updates::Idle(value)
  }
}

fn stay_awake_for(minutes: u32, cx: &mut App) -> Result<()> {
  ensure!(minutes > 0, "expected at least one minute");
  let duration = Duration::from_secs(u64::from(minutes) * 60);
  cx.update_global::<Idle, _>(|idle, cx| idle.set_stay_awake(true, Some(duration), cx));
  Ok(())
}

pub fn module(reads: &Subscriptions, subs: &mut Vec<Subscribe>) -> HostModule {
  Module::new("corona/idle")
    .func(read(
      reads,
      subs,
      "stayAwake",
      Updates::StayAwake,
      |cx| cx.idle().stay_awake.clone(),
      |cx| cx.idle().stay_awake(cx),
    ))
    .func(read(
      reads,
      subs,
      // staying awake, or an app holds idling back
      "inhibited",
      Updates::Inhibited,
      |cx| cx.idle().inhibited.clone(),
      |cx| cx.idle().inhibited(cx),
    ))
    .func(named!(
      "setStayAwake",
      /// No idle behaviors and no sleep until turned off.
      |cx: &mut App, on: bool| cx
        .update_global::<Idle, _>(|idle, cx| idle.set_stay_awake(on, None, cx))
    ))
    .func(named!(
      "stayAwakeFor",
      /// Stays awake, turned off again after `minutes`.
      |cx: &mut App, minutes: u32| stay_awake_for(minutes, cx)
    ))
    .into()
}

#[cfg(test)]
mod tests {
  use corona_utils::test_bus::{TestBus, wait_until};
  use futures_lite::future::block_on;
  use gpui_kit::{self as gpui, TestAppContext};
  use serde_json::json;

  use super::*;
  use crate::module::harness;

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
    cx.update(|cx| corona_idle::init(cx, &session, |_, _, _| {}));

    let body = r#"if (!globalThis.started) {
      globalThis.started = true;
      report(m.stayAwakeFor(0));
      m.stayAwakeFor(30);
    }
    report({ stayAwake: m.stayAwake(), inhibited: m.inhibited() });"#;
    let (view, cx) = harness::view(cx, body, |reads, subs, _| module(reads, subs));
    wait_until(cx, |_| view.reports.borrow().len() >= 2);
    assert_eq!(
      view.reports.borrow()[0],
      json!({ "message": "expected at least one minute" })
    );
    assert_eq!(view.last(), json!({ "stayAwake": true, "inhibited": true }));
    assert!(view.reads.contains(Updates::StayAwake.into()));
    assert!(view.reads.contains(Updates::Inhibited.into()));

    // turned off elsewhere, the view renders again
    cx.update(|_, cx| cx.update_global::<Idle, _>(|idle, cx| idle.set_stay_awake(false, None, cx)));
    wait_until(cx, |_| {
      view.last() == json!({ "stayAwake": false, "inhibited": false })
    });
  }
}
