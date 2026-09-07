use std::{env, ops::Deref, path::Path, rc::Rc};

use anyhow::{Context, Result, bail};
use gpui_kit::{App, Entity, Global};

use crate::compositor::{event::CompositorEventEmitter, hyprland::Hyprland, types::Workspace};

pub mod event;
mod hyprland;
pub mod types;

pub trait Compositor {
  fn emitter(&self) -> &Entity<CompositorEventEmitter>;

  fn list_workspaces(&self) -> Result<Vec<Workspace>>;
  fn active_workspace(&self) -> Result<Workspace>;

  fn list_monitors(&self) -> Result<Vec<types::Monitor>>;
  fn active_monitor(&self) -> Result<types::Monitor>;

  fn list_windows(&self) -> Result<Vec<types::Window>>;
  fn active_window(&self) -> Result<types::Window>;
}

pub struct CompositorRef(Rc<dyn Compositor>);

impl Global for CompositorRef {}

impl Deref for CompositorRef {
  type Target = dyn Compositor;

  fn deref(&self) -> &Self::Target {
    self.0.as_ref()
  }
}

pub trait CompositorExt {
  fn compositor(&self) -> &dyn Compositor;
}

impl CompositorExt for App {
  fn compositor(&self) -> &dyn Compositor {
    self.global::<CompositorRef>().0.as_ref()
  }
}

pub fn init(cx: &mut App) -> Result<()> {
  let runtime_dir = env::var("XDG_RUNTIME_DIR").context("XDG_RUNTIME_DIR is not set")?;

  let compositor: Rc<dyn Compositor> =
    if let Ok(hypr_instance) = env::var("HYPRLAND_INSTANCE_SIGNATURE") {
      let socket_dir = Path::new(&runtime_dir).join("hypr").join(hypr_instance);
      Rc::new(Hyprland::init(cx, &socket_dir))
    } else {
      bail!("Current compositor is not supported")
    };

  cx.set_global(CompositorRef(compositor));

  Ok(())
}
