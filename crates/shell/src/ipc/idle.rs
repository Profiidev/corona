use std::time::Duration;

use corona_idle::{Idle, IdleExt};
use anyhow::{Result, ensure};
use corona_ipc::{IpcCommand, IpcExt};
use gpui_kit::{App, BorrowAppContext};
use serde::{Deserialize, Serialize};

use corona_ipc::Switch;

pub(crate) fn register(cx: &mut App) {
  cx.ipc_mut().register::<StayAwake>();
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Request {
  pub switch: Switch,
  /// Turned off again after this many minutes, unset keeps it on
  pub minutes: Option<u32>,
}

/// Keeps the session awake: no idle behaviors, and logind neither idles nor sleeps
pub struct StayAwake;

impl IpcCommand for StayAwake {
  const COMMAND: &'static str = "stay_awake:switch";

  type Payload = Request;
  /// on or off afterwards
  type Response = bool;

  fn handle(request: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    ensure!(request.minutes != Some(0), "expected at least one minute");
    let current = cx.idle().stay_awake(cx);
    let Some(on) = request.switch.target(current) else {
      return Ok(current);
    };
    let duration = request
      .minutes
      .map(|minutes| Duration::from_secs(u64::from(minutes) * 60));
    cx.update_global::<Idle, _>(|idle, cx| idle.set_stay_awake(on, duration, cx));
    Ok(on)
  }
}

#[cfg(test)]
mod tests {
  use corona_utils::test_bus::TestBus;
  use gpui_kit::{self as gpui, TestAppContext};

  use super::*;

  #[gpui::test]
  fn switches_for_a_while(cx: &mut TestAppContext) {
    // the watcher finds no compositor, never the one running the tests
    let dir = tempfile::tempdir().unwrap();
    unsafe {
      std::env::set_var("XDG_RUNTIME_DIR", dir.path());
      std::env::set_var("WAYLAND_DISPLAY", "nothing-here");
    }
    cx.executor().allow_parking();
    let (_bus, session) = TestBus::with_conn();
    cx.update(|cx| corona_idle::start(cx, &session, |_, _, _| {}));
    let send = |switch, minutes, cx: &mut TestAppContext| {
      cx.update(|cx| StayAwake::handle(Request { switch, minutes }, cx))
    };

    assert!(send(Switch::On, Some(0), cx).is_err());
    assert!(!send(Switch::Status, None, cx).unwrap());
    assert!(send(Switch::Toggle, Some(30), cx).unwrap());
    assert!(send(Switch::Status, Some(5), cx).unwrap());
    cx.executor().advance_clock(Duration::from_secs(30 * 60));
    cx.run_until_parked();
    assert!(!send(Switch::Status, None, cx).unwrap());
    assert!(send(Switch::On, None, cx).unwrap());
    assert!(!send(Switch::Toggle, None, cx).unwrap());
    assert!(!cx.read(|cx| cx.idle().stay_awake(cx)));
  }
}
