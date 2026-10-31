//! A fake compositor and app setup for tests of the crates above
use std::{cell::RefCell, rc::Rc};

use anyhow::{Result, bail};
use corona_compositor::{Compositor, CompositorImpl, types};
use gpui_kit::TestAppContext;

pub fn workspace(id: &str, monitor: &str) -> types::Workspace {
  types::Workspace {
    id: id.into(),
    name: id.into(),
    monitor: monitor.into(),
    monitor_id: 0,
  }
}

pub fn monitor(name: &str) -> types::Monitor {
  types::Monitor {
    id: 0,
    name: name.into(),
    width: 1920,
    height: 1080,
    refresh_rate: 60.,
    x: 0,
    y: 0,
    active_scratchpad: None,
    active_workspace: workspace("1", name),
    scale: 1.,
    focused: true,
    disabled: false,
    mirror_of: "none".into(),
  }
}

/// A compositor answering from its fields and recording what it was asked
#[derive(Default)]
pub struct FakeCompositor {
  pub workspaces: Vec<types::Workspace>,
  pub windows: Vec<types::Window>,
  pub monitors: Vec<types::Monitor>,
  pub calls: RefCell<Vec<String>>,
  /// fails everything it is told to do, like a compositor that went away
  pub fail: bool,
}

impl FakeCompositor {
  fn call(&self, call: String) -> Result<()> {
    if self.fail {
      bail!("compositor down");
    }
    self.calls.borrow_mut().push(call);
    Ok(())
  }
}

impl CompositorImpl for FakeCompositor {
  fn list_workspaces(&self) -> Result<Vec<types::Workspace>> {
    Ok(self.workspaces.clone())
  }
  fn active_workspace(&self) -> Result<types::Workspace> {
    Ok(workspace("1", "DP-1"))
  }
  fn list_monitors(&self) -> Result<Vec<types::Monitor>> {
    Ok(self.monitors.clone())
  }
  fn active_monitor(&self) -> Result<types::Monitor> {
    Ok(monitor("DP-1"))
  }
  fn list_windows(&self) -> Result<Vec<types::Window>> {
    Ok(self.windows.clone())
  }
  fn active_window(&self) -> Result<Option<types::Window>> {
    Ok(self.windows.first().cloned())
  }
  fn focus_workspace(&self, workspace: &str) -> Result<()> {
    self.call(format!("workspace {workspace}"))
  }
  fn focus_window(&self, address: &str) -> Result<()> {
    self.call(format!("window {address}"))
  }
  fn close_window(&self, address: &str) -> Result<()> {
    self.call(format!("close {address}"))
  }
  fn cursor_position(&self) -> Result<(i32, i32)> {
    if self.fail {
      bail!("compositor down");
    }
    Ok((0, 0))
  }
  fn keyboard_layout(&self) -> Result<Option<String>> {
    Ok(None)
  }
  fn set_dpms(&self, on: bool) -> Result<()> {
    self.call(format!("dpms {on}"))
  }
  fn configure_monitor(&self, name: &str, change: types::MonitorChange) -> Result<()> {
    self.call(format!("monitor {name} {change:?}"))
  }
}

/// Keeps `corona_config::update` inside a temp dir
pub fn temp_config() -> tempfile::TempDir {
  let dir = tempfile::tempdir().unwrap();
  unsafe {
    std::env::set_var("XDG_CONFIG_HOME", dir.path().join("config"));
    std::env::set_var("XDG_STATE_HOME", dir.path().join("state"));
  }
  dir
}

/// gpui-kit, the default config and `fake` as the compositor
pub fn setup(fake: FakeCompositor, cx: &mut TestAppContext) -> Rc<FakeCompositor> {
  let fake = Rc::new(fake);
  cx.update(|cx| {
    gpui_kit::init(cx);
    cx.set_global(corona_config::Config::default());
    let compositor = Compositor::new(cx, fake.clone()).unwrap();
    cx.set_global(compositor);
  });
  fake
}
