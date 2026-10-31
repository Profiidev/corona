use corona_compositor::{self as cc, CompositorExt};
use corona_js::{Glob, Json, ModuleBuilder, PluginRef, ScriptModule, Throws, host_module};
use gpui_kit::App;
use serde::Serialize;
use specta::{Type, specta};

/// Switches to workspace `workspace`, by id or name.
#[specta]
fn focus_workspace(compositor: Glob<cc::Compositor>, workspace: String) -> Throws<()> {
  compositor.focus_workspace(&workspace).into()
}

#[specta]
fn focus_window(compositor: Glob<cc::Compositor>, address: String) -> Throws<()> {
  compositor.focus_window(&address).into()
}

#[specta]
fn close_window(compositor: Glob<cc::Compositor>, address: String) -> Throws<()> {
  compositor.close_window(&address).into()
}

/// In global layout coordinates.
#[derive(Serialize, Type)]
struct Position {
  x: i32,
  y: i32,
}

#[specta]
fn cursor_position(compositor: Glob<cc::Compositor>) -> Throws<Json<Position>> {
  (compositor.cursor_position())
    .map(|(x, y)| Json(Position { x, y }))
    .into()
}

/// `corona/compositor`
pub struct CompositorModule;

impl ScriptModule for CompositorModule {
  const NAME: &str = "compositor";
  const DOC: &str = "Workspaces, monitors and windows, and switching workspaces.";

  fn module(_: &PluginRef, m: &mut ModuleBuilder, _: &mut App) {
    m.watch(
      "listWorkspaces",
      "",
      |cx| cx.compositor().workspaces.clone(),
      |cx| Json(cx.compositor().list_workspaces(cx).to_vec()),
    );
    m.watch(
      "activeWorkspace",
      "",
      |cx| cx.compositor().active_workspace.clone(),
      |cx| Json(cx.compositor().active_workspace(cx).clone()),
    );
    m.watch(
      "listMonitors",
      "",
      |cx| cx.compositor().monitors.clone(),
      |cx| Json(cx.compositor().list_monitors(cx).to_vec()),
    );
    m.watch(
      "activeMonitor",
      "",
      |cx| cx.compositor().active_monitor.clone(),
      |cx| Json(cx.compositor().active_monitor(cx).clone()),
    );
    m.watch(
      "listWindows",
      "",
      |cx| cx.compositor().windows.clone(),
      |cx| Json(cx.compositor().list_windows(cx).to_vec()),
    );
    m.watch(
      "activeWindow",
      "The focused window; null when none is.",
      |cx| cx.compositor().active_window.clone(),
      |cx| Json(cx.compositor().active_window(cx).cloned()),
    );
    m.watch(
      "keyboardLayout",
      "The active keyboard layout's name; null when unknown.",
      |cx| cx.compositor().keyboard_layout.clone(),
      |cx| Json(cx.compositor().keyboard_layout(cx).map(str::to_string)),
    );
    host_module!(m, { sync: [focus_workspace] });
  }
}

/// `corona/compositor_control`: acting on other apps' windows and tracking the cursor.
pub struct CompositorControlModule;

impl ScriptModule for CompositorControlModule {
  const NAME: &str = "compositor_control";
  const DOC: &str = "Focusing and closing other apps' windows, and the cursor position.";
  const PRIVILEGED: bool = true;

  fn module(_: &PluginRef, m: &mut ModuleBuilder, _: &mut App) {
    host_module!(m, { sync: [focus_window, close_window, cursor_position] });
  }
}

#[cfg(test)]
mod tests {
  use std::rc::Rc;

  use anyhow::{Result, bail};
  use corona_compositor::{CompositorImpl, types};
  use gpui_kit::{self as gpui, TestAppContext};
  use serde_json::json;

  use super::*;
  use corona_script::harness;
  use corona_surface::testing::{FakeCompositor, setup};

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
      let compositor = cc::Compositor::new(cx, Rc::new(Down)).unwrap();
      cx.set_global(compositor);
    });
  }

  #[gpui::test]
  fn calls_every_compositor_function(cx: &mut TestAppContext) {
    let fake = setup(
      FakeCompositor {
        workspaces: vec![
          corona_surface::testing::workspace("1", "DP-1"),
          corona_surface::testing::workspace("2", "DP-1"),
        ],
        ..Default::default()
      },
      cx,
    );
    let body = r#"
    report({
      workspaces: m.listWorkspaces().map((w) => w.name),
      activeWorkspace: m.activeWorkspace().name,
      monitors: m.listMonitors().length,
      activeMonitor: m.activeMonitor().name,
      windows: m.listWindows().length,
      activeWindow: m.activeWindow(),
      keyboardLayout: m.keyboardLayout(),
      focusWorkspace: m.focusWorkspace("2"),
    });"#;
    let (view, cx) = harness::module::<CompositorModule>(cx, body);
    let last = view.last(cx);
    assert_eq!(last["workspaces"], json!(["1", "2"]));
    assert_eq!(last["activeWorkspace"], "1");
    assert_eq!(last["monitors"], 0);
    assert_eq!(last["activeMonitor"], "DP-1");
    assert_eq!(last["windows"], 0);
    assert!(last["activeWindow"].is_null());
    assert!(last["keyboardLayout"].is_null());
    assert!(fake.calls.borrow().contains(&"workspace 2".to_string()));
  }

  #[gpui::test]
  fn calls_every_control_function(cx: &mut TestAppContext) {
    let fake = setup(FakeCompositor::default(), cx);
    let body = r#"
    report({
      focusWindow: m.focusWindow("0xa"),
      closeWindow: m.closeWindow("0xb"),
      cursor: m.cursorPosition(),
    });"#;
    let (view, cx) = harness::module::<CompositorControlModule>(cx, body);
    let last = view.last(cx);
    // `undefined` is left out of the JSON
    assert!(last.get("focusWindow").is_none());
    assert!(last.get("closeWindow").is_none());
    assert_eq!(last["cursor"], json!({ "x": 0, "y": 0 }));
    let calls = fake.calls.borrow();
    assert!(calls.contains(&"window 0xa".to_string()), "{calls:?}");
    assert!(calls.contains(&"close 0xb".to_string()), "{calls:?}");
  }

  #[gpui::test]
  fn failures_and_bad_arguments_throw(cx: &mut TestAppContext) {
    install(cx);
    let body = r#"
    report({
      focusWorkspace: attempt(() => m.focusWorkspace("1")),
      // only in `corona/compositor_control`
      control: ["focusWindow", "closeWindow", "cursorPosition"].every((f) => !(f in m)),
      thrown: attempt(() => m.focusWorkspace(3)),
    });"#;
    let (view, cx) = harness::module::<CompositorModule>(cx, body);
    let last = view.last(cx);
    let down = json!({ "message": "compositor down" });
    assert_eq!(last["focusWorkspace"], down);
    assert_eq!(last["control"], true);
    assert!(last["thrown"]["message"].is_string(), "{last}");
  }

  #[gpui::test]
  fn control_fails_too(cx: &mut TestAppContext) {
    install(cx);
    let body = r#"
    report({
      focusWindow: attempt(() => m.focusWindow("0xa")),
      closeWindow: attempt(() => m.closeWindow("0xa")),
      cursorPosition: attempt(() => m.cursorPosition()),
    });"#;
    let (view, cx) = harness::module::<CompositorControlModule>(cx, body);
    let last = view.last(cx);
    for name in ["focusWindow", "closeWindow", "cursorPosition"] {
      assert_eq!(
        last[name],
        json!({ "message": "compositor down" }),
        "{name}"
      );
    }
  }

  #[gpui::test]
  fn renders_again_only_for_what_it_read(cx: &mut TestAppContext) {
    install(cx);
    let (view, cx) = harness::module::<CompositorModule>(cx, "report(m.activeWindow());");
    assert_eq!(view.last(cx), json!(null));

    let rendered = view.reports(cx).len();
    let (workspaces, active) = cx.update(|_, cx| {
      let c = cx.compositor();
      (c.workspaces.clone(), c.active_window.clone())
    });
    cx.update(|_, cx| workspaces.update(cx, |_, cx| cx.notify()));
    cx.run_until_parked();
    assert_eq!(view.reports(cx).len(), rendered);

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
      pid: 0,
    };
    cx.update(|_, cx| {
      active.update(cx, |active, cx| {
        *active = Some(window);
        cx.notify();
      })
    });
    cx.run_until_parked();
    assert_eq!(view.last(cx)["address"], "0xa");
  }
}
