use serde::Deserialize;

use crate::{data_cmd, integration::compositor::types};

data_cmd!(
  list_workspaces,
  "workspaces",
  Vec<Workspace>,
  Vec<types::Workspace>,
  |workspaces: Vec<Workspace>| { workspaces.into_iter().map(|w| w.into()).collect() }
);

data_cmd!(
  active_workspace,
  "activeworkspace",
  Workspace,
  types::Workspace,
  |workspace: Workspace| { workspace.into() }
);

#[derive(Debug, Deserialize)]
pub struct Workspace {
  pub id: u32,
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
