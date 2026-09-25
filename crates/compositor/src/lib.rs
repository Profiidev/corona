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
