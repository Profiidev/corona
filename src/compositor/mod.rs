use std::{env, ops::Deref, path::Path, rc::Rc};

use anyhow::{Context, Result, bail};
use gpui_kit::{App, Global};

use crate::compositor::{hyprland::Hyprland, types::Workspace};

mod hyprland;
mod types;

pub trait Compositor {
  fn list_workspaces(&self) -> Result<Vec<Workspace>>;
}

pub struct CompositorRef(Rc<dyn Compositor>);

impl Global for CompositorRef {}

impl Deref for CompositorRef {
  type Target = dyn Compositor;

  fn deref(&self) -> &Self::Target {
    self.0.as_ref()
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

  let res = compositor
    .list_workspaces()
    .context("Failed to list workspaces")?;
  dbg!("Workspaces: {:?}", res);

  cx.set_global(CompositorRef(compositor));

  Ok(())
}
