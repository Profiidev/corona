use corona_compositor::{Compositor, CompositorExt};
use gpui_kit::App;
use gpui_shell::HostModule;
use serde::Serialize;
use ts_rs::TS;

use crate::{
  host_fn::{Glob, Module},
  module::{Subscribe, Subscriptions, read},
};
use corona_macros::host_fn;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Updates {
  Workspaces,
  ActiveWorkspace,
  Monitors,
  ActiveMonitor,
  Windows,
  ActiveWindow,
  KeyboardLayout,
}

impl From<Updates> for super::Updates {
  fn from(value: Updates) -> Self {
    super::Updates::Compositor(value)
  }
}

#[host_fn]
fn focus_workspace(compositor: Glob<Compositor>, workspace: String) -> anyhow::Result<()> {
  compositor.focus_workspace(&workspace)
}

#[host_fn]
fn focus_window(compositor: Glob<Compositor>, address: String) -> anyhow::Result<()> {
  compositor.focus_window(&address)
}

#[host_fn]
fn close_window(compositor: Glob<Compositor>, address: String) -> anyhow::Result<()> {
  compositor.close_window(&address)
}

/// In global layout coordinates.
#[derive(Serialize, TS)]
struct Position {
  x: i32,
  y: i32,
}

#[host_fn]
fn cursor_position(compositor: Glob<Compositor>) -> anyhow::Result<Position> {
  let (x, y) = compositor.cursor_position()?;
  Ok(Position { x, y })
}

pub fn module(reads: &Subscriptions, subs: &mut Vec<Subscribe>, cx: &mut App) -> HostModule {
  let compositor = cx.compositor();

  Module::new("corona/compositor")
    .func(read(
      reads,
      subs,
      "listWorkspaces",
      Updates::Workspaces,
      compositor.workspaces.clone(),
      |cx| cx.compositor().list_workspaces(cx).to_vec(),
    ))
    .func(read(
      reads,
      subs,
      "activeWorkspace",
      Updates::ActiveWorkspace,
      compositor.active_workspace.clone(),
      |cx| cx.compositor().active_workspace(cx).clone(),
    ))
    .func(read(
      reads,
      subs,
      "listMonitors",
      Updates::Monitors,
      compositor.monitors.clone(),
      |cx| cx.compositor().list_monitors(cx).to_vec(),
    ))
    .func(read(
      reads,
      subs,
      "activeMonitor",
      Updates::ActiveMonitor,
      compositor.active_monitor.clone(),
      |cx| cx.compositor().active_monitor(cx).clone(),
    ))
    .func(read(
      reads,
      subs,
      "listWindows",
      Updates::Windows,
      compositor.windows.clone(),
      |cx| cx.compositor().list_windows(cx).to_vec(),
    ))
    .func(read(
      reads,
      subs,
      "activeWindow",
      Updates::ActiveWindow,
      compositor.active_window.clone(),
      |cx| cx.compositor().active_window(cx).cloned(),
    ))
    .func(read(
      reads,
      subs,
      "keyboardLayout",
      Updates::KeyboardLayout,
      compositor.keyboard_layout.clone(),
      |cx| cx.compositor().keyboard_layout(cx).map(str::to_string),
    ))
    .func(focus_workspace)
    .func(focus_window)
    .func(close_window)
    .func(cursor_position)
    .into()
}

#[cfg(test)]
mod tests {
  use std::rc::Rc;

  use anyhow::{Result, bail};
  use corona_compositor::{CompositorImpl, types};
  use gpui_kit::{self as gpui, TestAppContext};
  use serde_json::json;

  use super::*;
  use crate::module::harness;

  /// Has one workspace and no windows, fails everything it is told to do.
  struct Down;

  fn workspace() -> types::Workspace {
    types::Workspace {
      id: "1".into(),
      name: "1".into(),
      monitor: "DP-1".into(),
      monitor_id: 0,
    }
  }

  impl CompositorImpl for Down {
    fn list_workspaces(&self) -> Result<Vec<types::Workspace>> {
      Ok(vec![workspace()])
    }
    fn active_workspace(&self) -> Result<types::Workspace> {
      Ok(workspace())
    }
    fn list_monitors(&self) -> Result<Vec<types::Monitor>> {
      Ok(vec![])
    }
    fn active_monitor(&self) -> Result<types::Monitor> {
      Ok(types::Monitor {
        id: 0,
        name: "DP-1".into(),
        width: 1,
        height: 1,
        refresh_rate: 60.,
        x: 0,
        y: 0,
        active_scratchpad: None,
        active_workspace: workspace(),
        scale: 1.,
        focused: true,
        disabled: false,
        mirror_of: "none".into(),
      })
    }
    fn list_windows(&self) -> Result<Vec<types::Window>> {
      Ok(vec![])
    }
    fn active_window(&self) -> Result<Option<types::Window>> {
      Ok(None)
    }
    fn focus_workspace(&self, _: &str) -> Result<()> {
      bail!("compositor down")
    }
    fn focus_window(&self, _: &str) -> Result<()> {
      bail!("compositor down")
    }
    fn close_window(&self, _: &str) -> Result<()> {
      bail!("compositor down")
    }
    fn cursor_position(&self) -> Result<(i32, i32)> {
      bail!("compositor down")
    }
    fn keyboard_layout(&self) -> Result<Option<String>> {
      Ok(None)
    }
    fn set_dpms(&self, _: bool) -> Result<()> {
      bail!("compositor down")
    }
    fn configure_monitor(&self, _: &str, _: types::MonitorChange) -> Result<()> {
      bail!("compositor down")
    }
  }

  fn install(cx: &mut TestAppContext) {
    cx.update(|cx| {
      let compositor = Compositor::new(cx, Rc::new(Down)).unwrap();
      cx.set_global(compositor);
    });
  }

  #[gpui::test]
  fn failures_are_error_values_and_bad_arguments_throw(cx: &mut TestAppContext) {
    install(cx);
    let body = r#"
    let thrown = null;
    try { m.focusWorkspace(3); } catch (e) { thrown = String(e); }
    report({
      focusWorkspace: m.focusWorkspace("1"),
      focusWindow: m.focusWindow("0xa"),
      closeWindow: m.closeWindow("0xa"),
      cursorPosition: m.cursorPosition(),
      thrown,
    });"#;
    let (view, _) = harness::view(cx, body, module);
    let last = view.last();
    let down = json!({ "message": "compositor down" });
    for name in [
      "focusWorkspace",
      "focusWindow",
      "closeWindow",
      "cursorPosition",
    ] {
      assert_eq!(last[name], down, "{name}");
    }
    let thrown = last["thrown"].as_str().unwrap();
    assert!(thrown.contains("argument 0"), "{thrown}");
  }

  #[gpui::test]
  fn renders_again_only_for_what_it_read(cx: &mut TestAppContext) {
    install(cx);
    let (view, cx) = harness::view(cx, "report(m.activeWindow());", module);
    assert_eq!(view.last(), json!(null));
    let read = |update| {
      view
        .reads
        .contains(super::super::Updates::Compositor(update))
    };
    assert!(read(Updates::ActiveWindow));
    assert!(!read(Updates::Workspaces));

    let rendered = view.reports.borrow().len();
    let (workspaces, active) = cx.update(|_, cx| {
      let c = cx.compositor();
      (c.workspaces.clone(), c.active_window.clone())
    });
    cx.update(|_, cx| workspaces.update(cx, |_, cx| cx.notify()));
    cx.run_until_parked();
    assert_eq!(view.reports.borrow().len(), rendered);

    let window = types::Window {
      address: "0xa".into(),
      monitor: 0,
      workspace: "1".into(),
      class: "foot".into(),
      title: "shell".into(),
      x: 0,
      y: 0,
      width: 1,
      height: 1,
      floating: false,
      pinned: false,
      fullscreen: false,
      hidden: false,
      focus_history_id: 0,
    };
    cx.update(|_, cx| {
      active.update(cx, |active, cx| {
        *active = Some(window);
        cx.notify();
      })
    });
    cx.run_until_parked();
    assert_eq!(view.last()["address"], "0xa");
  }
}
