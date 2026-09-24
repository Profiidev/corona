use serde::Deserialize;

use crate::{hypr_data_cmd, hypr_dsp, integration::compositor::types};

hypr_data_cmd!(
  list_workspaces,
  "workspaces",
  Vec<Workspace>,
  Vec<types::Workspace>,
  |workspaces: Vec<Workspace>| {
    let mut workspaces: Vec<types::Workspace> = workspaces
      .into_iter()
      .filter(|w| w.workspace_type != "special")
      .map(|w| w.into())
      .collect();
    workspaces.sort_unstable_by_key(|w| w.id.clone());
    workspaces
  }
);

hypr_dsp!(
  focus_workspace,
  "focus({{ workspace = {} }})",
  workspace: &str
);

hypr_data_cmd!(
  active_workspace,
  "activeworkspace",
  Workspace,
  types::Workspace,
  |workspace: Workspace| { workspace.into() }
);

#[derive(Debug, Deserialize)]
pub struct Workspace {
  pub address: String,
  #[serde(rename = "type")]
  pub workspace_type: String,
  pub name: String,
  pub monitor: String,
  #[serde(rename = "monitorID")]
  pub monitor_id: u32,
}

impl From<Workspace> for types::Workspace {
  fn from(w: Workspace) -> Self {
    types::Workspace {
      id: w.address,
      name: w.name,
      monitor: w.monitor,
      monitor_id: w.monitor_id,
    }
  }
}
