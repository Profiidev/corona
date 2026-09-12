use serde::Deserialize;

use crate::{hypr_data_cmd, hypr_dsp, integration::compositor::types};

hypr_data_cmd!(
  list_workspaces,
  "workspaces",
  Vec<Workspace>,
  Vec<types::Workspace>,
  |workspaces: Vec<Workspace>| {
    workspaces
      .into_iter()
      .filter(|w| w.id >= 0)
      .map(|w| w.into())
      .collect()
  }
);

hypr_dsp!(
  focus_workspace,
  "focus({{ workspace = {} }})",
  workspace: i32
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
  pub id: i32,
  pub name: String,
  pub monitor: String,
  #[serde(rename = "monitorID")]
  pub monitor_id: u32,
}

impl From<Workspace> for types::Workspace {
  fn from(w: Workspace) -> Self {
    types::Workspace {
      id: w.id,
      name: w.name,
      monitor: w.monitor,
      monitor_id: w.monitor_id,
    }
  }
}
