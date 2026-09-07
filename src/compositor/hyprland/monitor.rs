use serde::Deserialize;

use crate::{compositor::types, data_cmd};

data_cmd!(
  list_monitors,
  "monitors",
  Vec<Monitor>,
  Vec<types::Monitor>,
  |monitors: Vec<Monitor>| { monitors.into_iter().map(|m| m.into()).collect() }
);

#[derive(Debug, Deserialize)]
pub struct Monitor {
  pub id: u32,
  pub name: String,
  pub width: u32,
  pub height: u32,
  #[serde(rename = "refreshRate")]
  pub refresh_rate: f32,
  pub x: i32,
  pub y: i32,
  #[serde(rename = "specialWorkspace")]
  pub special_workspace: WorkspaceInfo,
  #[serde(rename = "activeWorkspace")]
  pub active_workspace: WorkspaceInfo,
  pub scale: f32,
  pub focused: bool,
  pub disabled: bool,
  #[serde(rename = "mirrorOf")]
  pub mirror_of: String,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceInfo {
  pub id: u32,
  pub name: String,
}

impl From<Monitor> for types::Monitor {
  fn from(m: Monitor) -> Self {
    types::Monitor {
      id: m.id,
      name: m.name.clone(),
      width: m.width,
      height: m.height,
      refresh_rate: m.refresh_rate,
      x: m.x,
      y: m.y,
      active_scratchpad: (m.special_workspace.id != 0).then_some(types::Workspace {
        id: m.special_workspace.id,
        name: m.special_workspace.name,
        monitor: m.name.clone(),
        monitor_id: m.id,
      }),
      active_workspace: types::Workspace {
        id: m.active_workspace.id,
        name: m.active_workspace.name,
        monitor: m.name,
        monitor_id: m.id,
      },
      scale: m.scale,
      focused: m.focused,
      disabled: m.disabled,
      mirror_of: m.mirror_of,
    }
  }
}
