#[derive(Debug, Clone)]
pub struct Workspace {
  pub id: u32,
  pub name: String,
  pub monitor: String,
  pub monitor_id: u32,
}

#[derive(Debug, Clone)]
pub struct Monitor {
  pub id: u32,
  pub name: String,
  pub width: u32,
  pub height: u32,
  pub refresh_rate: f32,
  pub x: i32,
  pub y: i32,
  pub active_scratchpad: Option<Workspace>,
  pub active_workspace: Workspace,
  pub scale: f32,
  pub focused: bool,
  pub disabled: bool,
  pub mirror_of: String,
}
