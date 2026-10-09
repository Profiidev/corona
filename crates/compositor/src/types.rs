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

/// A change to one monitor's configuration. It lasts until the compositor
/// reloads its config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonitorChange {
  Enable,
  Disable,
  /// Show the monitor of this name on it
  Mirror(String),
  StopMirroring,
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

  #[test]
  fn stacking_order() {
    let mut pinned = window("1", false, 9);
    pinned.pinned = true;
    let mut fullscreen = window("1", false, 9);
    fullscreen.fullscreen = true;
    let floating = window("1", true, 0);
    let special = window("special:x", false, 9);
    // a workspace merely named special is not one
    let named = window("special", true, 0);
    assert!(special.stacking() > pinned.stacking());
    assert!(pinned.stacking() > fullscreen.stacking());
    assert!(fullscreen.stacking() > floating.stacking());
    assert!(named.stacking() == floating.stacking());
    assert!(window("1", false, 0).stacking() == window("2", false, 0).stacking());
  }

  #[test]
  fn stacking_negative_focus_history_and_tie_breaks() {
    let unmapped = window("1", false, -1);
    let focused = window("1", false, 0);
    let unfocused_tied1 = window("1", false, 2);
    let unfocused_tied2 = window("1", false, 2);

    // Negative ID (-1) under Reverse(-1) compares greater than Reverse(0)
    assert!(unmapped.stacking() > focused.stacking());

    // Identical stacking properties result in total equality (tie)
    assert!(unfocused_tied1.stacking() == unfocused_tied2.stacking());
  }

  #[test]
  fn display_ids_follow_the_monitor() {
    let workspace = |monitor: &str| super::Workspace {
      id: "0x1".into(),
      name: "1".into(),
      monitor: monitor.into(),
      monitor_id: 0,
    };
    assert_eq!(
      workspace("DP-1").display_id(),
      corona_utils::display::display_uuid("DP-1")
    );
    assert_ne!(
      workspace("DP-1").display_id(),
      workspace("DP-2").display_id()
    );
  }

  #[test]
  fn serializes_for_scripts() {
    let mut w = window("1", false, 0);
    w.address = "0xa".into();
    let json = serde_json::to_value(&w).unwrap();
    assert_eq!(json["focus_history_id"], 0);
    assert_eq!(json["address"], "0xa");
  }
}
