use std::{collections::HashSet, rc::Rc};

use anyhow::Result;
use corona_utils::entity::WriteChangedExt;
use gpui_kit::{App, AppContext, Entity, Global};

use crate::types;

pub struct Compositor {
  inner: Rc<dyn CompositorImpl>,
  pub workspaces: Entity<Vec<types::Workspace>>,
  pub active_workspace: Entity<types::Workspace>,
  pub monitors: Entity<Vec<types::Monitor>>,
  pub active_monitor: Entity<types::Monitor>,
  pub windows: Entity<Vec<types::Window>>,
  pub active_window: Entity<Option<types::Window>>,
  pub urgent: Entity<HashSet<String>>,
  pub keyboard_layout: Entity<Option<String>>,
}

impl Global for Compositor {}

fn init_state<T: 'static>(cx: &mut App, f: impl FnOnce() -> Result<T>) -> Result<Entity<T>> {
  let data = f()?;
  Ok(cx.new(|_| data))
}

impl Compositor {
  pub(crate) fn new(cx: &mut App, inner: Rc<dyn CompositorImpl>) -> Result<Self> {
    let workspaces = init_state(cx, || inner.list_workspaces())?;
    let active_workspace = init_state(cx, || inner.active_workspace())?;
    let monitors = init_state(cx, || inner.list_monitors())?;
    let active_monitor = init_state(cx, || inner.active_monitor())?;
    let windows = init_state(cx, || inner.list_windows())?;
    let active_window = init_state(cx, || inner.active_window())?;
    let urgent = cx.new(|_| HashSet::new());
    let keyboard_layout = init_state(cx, || inner.keyboard_layout())?;

    Ok(Self {
      inner,
      workspaces,
      active_workspace,
      monitors,
      active_monitor,
      windows,
      active_window,
      urgent,
      keyboard_layout,
    })
  }

  /// required when relying on size and position of windows, no event when they change
  pub fn refresh_windows(cx: &mut App) -> Result<()> {
    let compositor = cx.global::<Compositor>();
    let windows = compositor.inner.list_windows()?;
    compositor.windows.clone().write_changed(cx, windows);
    Ok(())
  }

  pub fn focus_workspace(&self, workspace: &str) -> Result<()> {
    self.inner.focus_workspace(workspace)
  }

  pub fn focus_window(&self, address: &str) -> Result<()> {
    self.inner.focus_window(address)
  }

  pub fn close_window(&self, address: &str) -> Result<()> {
    self.inner.close_window(address)
  }

  /// Turns every monitor's power on or off
  pub fn set_dpms(&self, on: bool) -> Result<()> {
    self.inner.set_dpms(on)
  }

  pub fn cursor_position(&self) -> Result<(i32, i32)> {
    self.inner.cursor_position()
  }

  pub fn list_workspaces<'c>(&self, cx: &'c App) -> &'c [types::Workspace] {
    self.workspaces.read(cx)
  }

  pub fn active_workspace<'c>(&self, cx: &'c App) -> &'c types::Workspace {
    self.active_workspace.read(cx)
  }

  pub fn list_monitors<'c>(&self, cx: &'c App) -> &'c [types::Monitor] {
    self.monitors.read(cx)
  }

  pub fn active_monitor<'c>(&self, cx: &'c App) -> &'c types::Monitor {
    self.active_monitor.read(cx)
  }

  pub fn list_windows<'c>(&self, cx: &'c App) -> &'c [types::Window] {
    self.windows.read(cx)
  }

  pub fn active_window<'c>(&self, cx: &'c App) -> Option<&'c types::Window> {
    self.active_window.read(cx).as_ref()
  }

  pub fn keyboard_layout<'c>(&self, cx: &'c App) -> Option<&'c str> {
    self.keyboard_layout.read(cx).as_deref()
  }

  pub fn is_urgent(&self, address: &str, cx: &App) -> bool {
    self.urgent.read(cx).contains(address)
  }
}

pub(crate) trait CompositorImpl {
  fn list_workspaces(&self) -> Result<Vec<types::Workspace>>;
  fn active_workspace(&self) -> Result<types::Workspace>;

  fn list_monitors(&self) -> Result<Vec<types::Monitor>>;
  fn active_monitor(&self) -> Result<types::Monitor>;

  fn list_windows(&self) -> Result<Vec<types::Window>>;
  fn active_window(&self) -> Result<Option<types::Window>>;

  fn focus_workspace(&self, workspace: &str) -> Result<()>;
  fn focus_window(&self, address: &str) -> Result<()>;
  fn close_window(&self, address: &str) -> Result<()>;

  fn cursor_position(&self) -> Result<(i32, i32)>;

  fn keyboard_layout(&self) -> Result<Option<String>>;

  fn set_dpms(&self, on: bool) -> Result<()>;
}

pub trait CompositorExt {
  fn compositor(&self) -> &Compositor;
}

impl CompositorExt for App {
  fn compositor(&self) -> &Compositor {
    self.global::<Compositor>()
  }
}

#[cfg(test)]
mod tests {
  use std::{cell::RefCell, path::Path};

  use gpui_kit::{self as gpui, TestAppContext};

  use super::*;
  use crate::hyprland::{Hyprland, fake::FakeHyprland};

  fn workspace(name: &str) -> types::Workspace {
    types::Workspace {
      id: format!("0x{name}"),
      name: name.into(),
      monitor: "DP-1".into(),
      monitor_id: 0,
    }
  }

  /// answers from fields, records what it was asked to do
  #[derive(Default)]
  struct Fake {
    windows: RefCell<Vec<types::Window>>,
    calls: RefCell<Vec<String>>,
    fail: bool,
  }

  impl CompositorImpl for Fake {
    fn list_workspaces(&self) -> Result<Vec<types::Workspace>> {
      Ok(vec![workspace("1"), workspace("2")])
    }
    fn active_workspace(&self) -> Result<types::Workspace> {
      Ok(workspace("1"))
    }
    fn list_monitors(&self) -> Result<Vec<types::Monitor>> {
      Ok(vec![])
    }
    fn active_monitor(&self) -> Result<types::Monitor> {
      if self.fail {
        anyhow::bail!("No active monitor found");
      }
      Ok(types::Monitor {
        id: 0,
        name: "DP-1".into(),
        width: 1,
        height: 1,
        refresh_rate: 60.,
        x: 0,
        y: 0,
        active_scratchpad: None,
        active_workspace: workspace("1"),
        scale: 1.,
        focused: true,
        disabled: false,
        mirror_of: "none".into(),
      })
    }
    fn list_windows(&self) -> Result<Vec<types::Window>> {
      Ok(self.windows.borrow().clone())
    }
    fn active_window(&self) -> Result<Option<types::Window>> {
      Ok(None)
    }
    fn focus_workspace(&self, workspace: &str) -> Result<()> {
      self
        .calls
        .borrow_mut()
        .push(format!("workspace {workspace}"));
      Ok(())
    }
    fn focus_window(&self, address: &str) -> Result<()> {
      self.calls.borrow_mut().push(format!("focus {address}"));
      Ok(())
    }
    fn close_window(&self, address: &str) -> Result<()> {
      self.calls.borrow_mut().push(format!("close {address}"));
      Ok(())
    }
    fn cursor_position(&self) -> Result<(i32, i32)> {
      Ok((1, 2))
    }
    fn keyboard_layout(&self) -> Result<Option<String>> {
      Ok(None)
    }
    fn set_dpms(&self, on: bool) -> Result<()> {
      self.calls.borrow_mut().push(format!("dpms {on}"));
      Ok(())
    }
  }

  #[gpui::test]
  fn forwards_and_reads(cx: &mut TestAppContext) {
    let fake = Rc::new(Fake::default());
    cx.update(|cx| {
      let compositor = Compositor::new(cx, fake.clone()).unwrap();
      cx.set_global(compositor);
      let compositor = cx.compositor();
      compositor.focus_workspace("2").unwrap();
      compositor.focus_window("0xa").unwrap();
      compositor.close_window("0xb").unwrap();
      compositor.set_dpms(false).unwrap();
      assert_eq!(compositor.cursor_position().unwrap(), (1, 2));
      assert_eq!(compositor.list_workspaces(cx).len(), 2);
      assert_eq!(compositor.active_workspace(cx).name, "1");
      assert_eq!(compositor.active_monitor(cx).name, "DP-1");
      assert!(compositor.list_monitors(cx).is_empty());
      assert!(compositor.list_windows(cx).is_empty());
      assert_eq!(compositor.active_window(cx), None);
      assert_eq!(compositor.keyboard_layout(cx), None);
      assert!(!compositor.is_urgent("0xa", cx));
    });
    assert_eq!(
      *fake.calls.borrow(),
      ["workspace 2", "focus 0xa", "close 0xb", "dpms false"]
    );
  }

  #[gpui::test]
  fn new_fails_on_any_query(cx: &mut TestAppContext) {
    let fake = Rc::new(Fake {
      fail: true,
      ..Default::default()
    });
    cx.update(|cx| {
      let error = Compositor::new(cx, fake).err().unwrap();
      assert_eq!(error.to_string(), "No active monitor found");
    });
  }

  #[gpui::test]
  fn refresh_windows_notifies_on_change(cx: &mut TestAppContext) {
    let fake = Rc::new(Fake::default());
    let windows = cx.update(|cx| {
      let compositor = Compositor::new(cx, fake.clone()).unwrap();
      let windows = compositor.windows.clone();
      cx.set_global(compositor);
      windows
    });
    let notified = Rc::new(std::cell::Cell::new(0));
    let count = notified.clone();
    cx.update(|cx| {
      cx.observe(&windows, move |_, _| count.set(count.get() + 1))
        .detach()
    });
    cx.update(|cx| Compositor::refresh_windows(cx).unwrap());
    cx.run_until_parked();
    assert_eq!(notified.get(), 0);
    let hypr = FakeHyprland::start();
    let listed = hypr.ipc().list_windows().unwrap();
    *fake.windows.borrow_mut() = listed.clone();
    cx.update(|cx| Compositor::refresh_windows(cx).unwrap());
    cx.run_until_parked();
    assert_eq!(notified.get(), 1);
    cx.read(|cx| assert_eq!(cx.compositor().list_windows(cx), listed));
  }

  #[gpui::test]
  fn hyprland_backend(cx: &mut TestAppContext) {
    let hypr = FakeHyprland::start();
    cx.update(|cx| {
      let backend = Rc::new(Hyprland::init(cx, &hypr.dir));
      let compositor = Compositor::new(cx, backend).unwrap();
      cx.set_global(compositor);
      let compositor = cx.compositor();
      // the focused, enabled monitor
      assert_eq!(compositor.active_monitor(cx).name, "eDP-1");
      assert_eq!(compositor.keyboard_layout(cx), Some("German"));
      assert_eq!(compositor.active_window(cx).unwrap().address, "0xa");
      assert_eq!(compositor.cursor_position().unwrap(), (-5, 1200));
      compositor.set_dpms(true).unwrap();
      compositor.set_dpms(false).unwrap();
    });
    let dpms: Vec<_> = hypr
      .commands()
      .into_iter()
      .filter(|c| c.contains("dpms"))
      .collect();
    assert_eq!(
      dpms,
      [
        r#"/eval hl.dispatch(hl.dsp.dpms({ action = "on" }))"#,
        r#"/eval hl.dispatch(hl.dsp.dpms({ action = "off" }))"#,
      ]
    );
  }

  #[gpui::test]
  fn hyprland_without_an_active_monitor(cx: &mut TestAppContext) {
    let hypr = FakeHyprland::start();
    let all_disabled =
      crate::hyprland::fake::MONITORS.replace(r#""focused": true"#, r#""focused": false"#);
    hypr.answer("j/monitors all", all_disabled);
    cx.update(|cx| {
      let backend = Rc::new(Hyprland::init(cx, Path::new(&hypr.dir)));
      let error = Compositor::new(cx, backend).err().unwrap();
      assert_eq!(error.to_string(), "No active monitor found");
    });
    let focused_but_off = crate::hyprland::fake::MONITORS.replace(
      r#""disabled": false, "mirrorOf": "none"}
]"#,
      r#""disabled": true, "mirrorOf": "none"}
]"#,
    );
    hypr.answer("j/monitors all", focused_but_off);
    cx.update(|cx| {
      let backend = Rc::new(Hyprland::init(cx, Path::new(&hypr.dir)));
      assert!(Compositor::new(cx, backend).is_err());
    });
  }
}
