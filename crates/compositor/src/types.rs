use corona_utils::display::display_uuid;
use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, TS)]
pub struct Workspace {
  pub id: String,
  pub name: String,
  pub monitor: String,
  pub monitor_id: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct Monitor {
  pub id: u32,
  pub name: String,
  pub width: u32,
  pub height: u32,
  pub refresh_rate: f32,
  pub x: i32,
  pub y: i32,
  #[serde(skip_serializing_if = "Option::is_none")]
  #[ts(optional)]
  pub active_scratchpad: Option<Workspace>,
  pub active_workspace: Workspace,
  pub scale: f32,
  pub focused: bool,
  pub disabled: bool,
  pub mirror_of: String,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct Window {
  pub address: String,
  pub monitor: u32,
  pub workspace: String,
  pub class: String,
  pub title: String,
  pub x: i32,
  pub y: i32,
}

impl Workspace {
  pub fn display_id(&self) -> Uuid {
    display_uuid(&self.monitor)
  }
}
