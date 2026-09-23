use std::{env, path::Path, rc::Rc};

use anyhow::{Context, Result, bail};
use gpui_kit::{App, AppContext, Entity, Global};

use crate::integration::compositor::hyprland::Hyprland;

mod hyprland;
pub mod types;

pub struct Compositor {
  inner: Rc<dyn CompositorImpl>,
  pub workspaces: Entity<Vec<types::Workspace>>,
  pub active_workspace: Entity<types::Workspace>,
  pub monitors: Entity<Vec<types::Monitor>>,
  pub active_monitor: Entity<types::Monitor>,
  pub windows: Entity<Vec<types::Window>>,
  pub active_window: Entity<Option<types::Window>>,
}

impl Global for Compositor {}

fn init_state<T: 'static>(cx: &mut App, f: impl FnOnce() -> Result<T>) -> Result<Entity<T>> {
  let data = f()?;
  Ok(cx.new(|_| data))
}

impl Compositor {
  fn new(cx: &mut App, inner: Rc<dyn CompositorImpl>) -> Result<Self> {
    let workspaces = init_state(cx, || inner.list_workspaces())?;
    let active_workspace = init_state(cx, || inner.active_workspace())?;
    let monitors = init_state(cx, || inner.list_monitors())?;
    let active_monitor = init_state(cx, || inner.active_monitor())?;
    let windows = init_state(cx, || inner.list_windows())?;
    let active_window = init_state(cx, || inner.active_window())?;

    Ok(Self {
      inner,
      workspaces,
      active_workspace,
      monitors,
      active_monitor,
      windows,
      active_window,
    })
  }

  pub fn focus_workspace(&self, workspace: &str) -> Result<()> {
    self.inner.focus_workspace(workspace)
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
}

trait CompositorImpl {
  fn list_workspaces(&self) -> Result<Vec<types::Workspace>>;
  fn active_workspace(&self) -> Result<types::Workspace>;

  fn list_monitors(&self) -> Result<Vec<types::Monitor>>;
  fn active_monitor(&self) -> Result<types::Monitor>;

  fn list_windows(&self) -> Result<Vec<types::Window>>;
  fn active_window(&self) -> Result<Option<types::Window>>;

  fn focus_workspace(&self, workspace: &str) -> Result<()>;
}

pub trait CompositorExt {
  fn compositor(&self) -> &Compositor;
}

impl CompositorExt for App {
  fn compositor(&self) -> &Compositor {
    self.global::<Compositor>()
  }
}

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
