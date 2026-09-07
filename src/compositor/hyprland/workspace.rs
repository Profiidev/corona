use serde::Deserialize;

use crate::{compositor::types, data_cmd};

data_cmd!(get_workspaces, "workspaces", Vec<Workspace>);

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
