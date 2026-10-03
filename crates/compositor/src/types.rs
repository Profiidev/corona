use corona_utils::display::display_uuid;
use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct Workspace {
  pub id: String,
  pub name: String,
  pub monitor: String,
  pub monitor_id: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
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

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct Window {
  pub address: String,
  pub monitor: u32,
  pub workspace: String,
  pub class: String,
  pub title: String,
  pub x: i32,
  pub y: i32,
  pub width: i32,
  pub height: i32,
  pub floating: bool,
  pub pinned: bool,
  pub fullscreen: bool,
  pub hidden: bool,
  pub focus_history_id: i32,
}

impl Window {
  /// Draw order: special workspaces > pinned > fullscreen > floating > focus history
  pub fn stacking(&self) -> impl Ord + use<> {
    (
      self.workspace.starts_with("special:"),
      self.pinned,
      self.fullscreen,
      self.floating,
      std::cmp::Reverse(self.focus_history_id),
    )
  }
}

impl Workspace {
  pub fn display_id(&self) -> Uuid {
    display_uuid(&self.monitor)
  }
}

#[cfg(test)]
mod tests {
  use super::Window;

  fn window(workspace: &str, floating: bool, focus_history_id: i32) -> Window {
    Window {
      address: String::new(),
      monitor: 0,
      workspace: workspace.into(),
      class: String::new(),
      title: String::new(),
      x: 0,
      y: 0,
      width: 10,
      height: 10,
      floating,
      pinned: false,
      fullscreen: false,
      hidden: false,
      focus_history_id,
    }
  }

  #[test]
  fn stacking() {
    let tiled_focused = window("1", false, 0);
    let floating_old = window("1", true, 5);
    let floating_recent = window("1", true, 1);
    let special = window("special:magic", false, 9);

    assert!(floating_old.stacking() > tiled_focused.stacking());
    assert!(floating_recent.stacking() > floating_old.stacking());
    assert!(special.stacking() > floating_recent.stacking());
  }
}
