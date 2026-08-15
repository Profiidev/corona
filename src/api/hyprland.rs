use hyprland::{
  data::{Workspace, Workspaces},
  dispatch::{Dispatch, WorkspaceIdentifierWithSpecial},
  shared::HyprData,
};

use crate::Corona;

impl Corona {
  pub fn workspace_list(&self) -> hyprland::Result<Vec<Workspace>> {
    Ok(Workspaces::get()?.into_iter().collect())
  }

  pub fn dispatch_workspace(&self, workspace: String) -> hyprland::Result<()> {
    Dispatch::call(hyprland::dispatch::DispatchType::Workspace(
      WorkspaceIdentifierWithSpecial::Name(&workspace),
    ))
  }
}
