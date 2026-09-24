use serde::Deserialize;

use crate::{hypr_data_cmd, integration::compositor::types};

hypr_data_cmd!(
  list_monitors,
  "monitors all",
  Vec<Monitor>,
  Vec<types::Monitor>,
  |monitors: Vec<Monitor>| {
    let mut monitors: Vec<types::Monitor> = monitors.into_iter().map(|m| m.into()).collect();
    monitors.sort_unstable_by(|a, b| a.x.cmp(&b.x).then_with(|| a.y.cmp(&b.y)));
    monitors
  }
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
  pub address: String,
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
      active_scratchpad: (!m.special_workspace.address.is_empty()).then_some(types::Workspace {
        id: m.special_workspace.address,
        name: m.special_workspace.name,
        monitor: m.name.clone(),
        monitor_id: m.id,
      }),
      active_workspace: types::Workspace {
        id: m.active_workspace.address,
        name: m.active_workspace.name,
        monitor: m.name,
        monitor_id: m.id,
      },
      scale: m.scale,
      focused: m.focused,
      // Hyprland 0.56 JSON has `disabled` inverted: enabled monitors report
      // true while the plain `hyprctl monitors` output says false.
      disabled: !m.disabled,
      mirror_of: m.mirror_of,
    }
  }
}
