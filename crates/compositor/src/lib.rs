use std::{env, path::Path, rc::Rc};

use anyhow::{Context, Result, bail};
use gpui_kit::App;

use crate::{hyprland::Hyprland, state::CompositorImpl};

pub use state::{Compositor, CompositorExt};

mod hyprland;
mod state;
pub mod types;

pub fn init(cx: &mut App) -> Result<()> {
  let runtime_dir = env::var("XDG_RUNTIME_DIR").context("XDG_RUNTIME_DIR is not set")?;

  let inner: Rc<dyn CompositorImpl> =
    if let Ok(hypr_instance) = env::var("HYPRLAND_INSTANCE_SIGNATURE") {
      let socket_dir = Path::new(&runtime_dir).join("hypr").join(hypr_instance);
      Rc::new(Hyprland::init(cx, &socket_dir))
    } else {
      bail!("Current compositor is not supported")
    };

  let compositor = Compositor::new(cx, inner)?;
  cx.set_global(compositor);

  Ok(())
}

#[cfg(test)]
mod tests {
  use std::{
    thread,
    time::{Duration, Instant},
  };

  use gpui_kit::{self as gpui, TestAppContext};

  use super::*;
  use crate::hyprland::fake::FakeHyprland;

  /// nextest runs each test in its own process, so the environment is ours
  fn point_at(hypr: &FakeHyprland) {
    unsafe {
      env::set_var("XDG_RUNTIME_DIR", hypr.runtime_dir());
      env::set_var("HYPRLAND_INSTANCE_SIGNATURE", "sig");
    }
  }

  /// the listener is a real thread: wait in real time
  fn wait(cx: &mut TestAppContext, done: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
      cx.run_until_parked();
      if cx.read(&done) {
        return;
      }
      assert!(Instant::now() < deadline, "timed out");
      thread::sleep(Duration::from_millis(5));
    }
  }

  fn start(cx: &mut TestAppContext) -> FakeHyprland {
    cx.executor().allow_parking();
    let hypr = FakeHyprland::start();
    point_at(&hypr);
    cx.update(|cx| init(cx).unwrap());
    let deadline = Instant::now() + Duration::from_secs(10);
    while !hypr.connected() {
      assert!(Instant::now() < deadline, "listener never connected");
      thread::sleep(Duration::from_millis(5));
    }
    hypr
  }

  #[gpui::test]
  fn needs_hyprland(cx: &mut TestAppContext) {
    unsafe {
      env::remove_var("XDG_RUNTIME_DIR");
      env::remove_var("HYPRLAND_INSTANCE_SIGNATURE");
    }
    cx.update(|cx| {
      assert_eq!(
        init(cx).unwrap_err().to_string(),
        "XDG_RUNTIME_DIR is not set"
      );
      unsafe { env::set_var("XDG_RUNTIME_DIR", "/tmp") };
      assert_eq!(
        init(cx).unwrap_err().to_string(),
        "Current compositor is not supported"
      );
      assert!(!cx.has_global::<Compositor>());
    });
  }

  #[gpui::test]
  fn events_update_the_state(cx: &mut TestAppContext) {
    let hypr = start(cx);
    cx.read(|cx| assert_eq!(cx.compositor().active_workspace(cx).name, "1"));

    hypr.push("urgent>>b");
    wait(cx, |cx| cx.compositor().is_urgent("0xb", cx));
    hypr.push("activewindowv2>>b");
    wait(cx, |cx| !cx.compositor().is_urgent("0xb", cx));

    // a new workspace: the next queries see it
    hypr.answer(
      "j/activeworkspace",
      r#"{"address": "0x2", "type": "normal", "name": "2", "monitor": "DP-1", "monitorID": 1}"#,
    );
    hypr.push("workspace>>2");
    wait(cx, |cx| cx.compositor().active_workspace(cx).name == "2");

    hypr.answer("j/activewindow", "{}");
    hypr.push("closewindow>>a");
    wait(cx, |cx| cx.compositor().active_window(cx).is_none());

    hypr.answer(
      "j/devices",
      r#"{"keyboards": [{"active_keymap": "French", "main": true}]}"#,
    );
    hypr.push("activelayout>>kbd,French");
    wait(cx, |cx| {
      cx.compositor().keyboard_layout(cx) == Some("French")
    });

    hypr.push("focusedmon>>DP-1,2");
    wait(cx, |cx| cx.compositor().active_monitor(cx).name == "DP-1");
  }

  #[gpui::test]
  fn bad_lines_are_skipped(cx: &mut TestAppContext) {
    let hypr = start(cx);
    hypr.push("not an event");
    hypr.push("focusedmon>>nowhere,1");
    hypr.push("urgent>>c");
    wait(cx, |cx| cx.compositor().is_urgent("0xc", cx));
  }

  #[gpui::test]
  fn reconnects_after_a_hang_up(cx: &mut TestAppContext) {
    let hypr = start(cx);
    hypr.hang_up();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !hypr.connected() {
      assert!(Instant::now() < deadline, "never reconnected");
      thread::sleep(Duration::from_millis(5));
    }
    hypr.push("urgent>>d");
    wait(cx, |cx| cx.compositor().is_urgent("0xd", cx));
  }

  #[gpui::test]
  fn unchanged_state_does_not_notify(cx: &mut TestAppContext) {
    let hypr = start(cx);
    let workspaces = cx.read(|cx| cx.compositor().workspaces.clone());
    let urgent = cx.read(|cx| cx.compositor().urgent.clone());
    let notified = std::rc::Rc::new(std::cell::Cell::new(0));
    let count = notified.clone();
    cx.update(|cx| {
      cx.observe(&workspaces, move |_, _| count.set(count.get() + 1))
        .detach()
    });
    let count = notified.clone();
    cx.update(|cx| {
      cx.observe(&urgent, move |_, _| count.set(count.get() + 1))
        .detach()
    });
    // same answers as before: nothing changed
    hypr.push("workspace>>1");
    hypr.push("activewindowv2>>nothing-urgent");
    hypr.push("urgent>>e");
    wait(cx, |cx| cx.compositor().is_urgent("0xe", cx));
    hypr.push("urgent>>e");
    hypr.push("urgent>>f");
    wait(cx, |cx| cx.compositor().is_urgent("0xf", cx));
    assert_eq!(notified.get(), 2);
  }
}
