use hyprland::{
  data::{Workspace, Workspaces},
  shared::HyprData,
};

use crate::Corona;

impl Corona {
  pub fn workspace_list(&self) -> hyprland::Result<Vec<Workspace>> {
    Ok(Workspaces::get()?.into_iter().collect())
  }
}
