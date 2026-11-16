use corona_utils::display::display_uuid;
use serde::Serialize;
use specta::Type;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
pub struct Workspace {
  pub id: String,
  pub name: String,
  pub monitor: String,
  pub monitor_id: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
pub struct Monitor {
  pub id: u32,
  pub name: String,
  pub width: u32,
  pub height: u32,
  pub refresh_rate: f32,
  pub x: i32,
  pub y: i32,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub active_scratchpad: Option<Workspace>,
  pub active_workspace: Workspace,
  pub scale: f32,
  pub focused: bool,
  pub disabled: bool,
  pub mirror_of: String,
  /// Space layer surfaces reserve, as left, top, right, bottom
  pub reserved: [i32; 4],
  pub hardware: Hardware,
}

/// A video mode a monitor offers
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Type)]
pub struct Mode {
  pub width: u32,
  pub height: u32,
  pub refresh_rate: f32,
}

impl Mode {
  /// `1920x1200@60.00Hz`, as Hyprland lists it
  pub fn parse(text: &str) -> Option<Self> {
    let (size, rate) = text.split_once('@')?;
    let (width, height) = size.split_once('x')?;
    Some(Self {
      width: width.parse().ok()?,
      height: height.parse().ok()?,
      refresh_rate: rate.trim_end_matches("Hz").parse().ok()?,
    })
  }

  /// Refresh rate in whole millihertz, to tell modes apart
  pub fn millihertz(&self) -> u32 {
    (self.refresh_rate * 1000.).round() as u32
  }
}

/// What the monitor is, not how it is set up
#[derive(Debug, Clone, Default, PartialEq, Serialize, Type)]
pub struct Hardware {
  /// Panel size in millimetres, `(0, 0)` when it does not say
  pub physical_mm: (u32, u32),
  /// Hyprland's transform: 0-3 rotate, 4-7 rotate and flip
  pub transform: i32,
  pub modes: Vec<Mode>,
}

impl Monitor {
  /// The size in the layout, and so for the cursor: the mode over the scale,
  /// turned sideways when rotated by 90° or 270°
  pub fn logical_size(&self) -> (i32, i32) {
    logical_size(
      (self.width, self.height),
      self.scale,
      self.hardware.transform,
    )
  }

  /// Pixels per inch along the width, if the panel reports its size
  pub fn dpi(&self) -> Option<f32> {
    let (mm, _) = self.hardware.physical_mm;
    let pixels = match self.hardware.transform % 2 {
      0 => self.width,
      _ => self.height,
    };
    (mm > 0).then(|| pixels as f32 / (mm as f32 / 25.4))
  }
}

/// A mode, `scale` and `transform` as Hyprland lays them out
pub fn logical_size(mode: (u32, u32), scale: f32, transform: i32) -> (i32, i32) {
  let scale = if scale > 0. { scale } else { 1. };
  let (w, h) = (
    (mode.0 as f32 / scale).round() as i32,
    (mode.1 as f32 / scale).round() as i32,
  );
  match transform.rem_euclid(2) {
    1 => (h, w),
    _ => (w, h),
  }
}

/// The monitor in focus, none for a moment while the compositor has none
pub(crate) fn focused_monitor(monitors: &[Monitor]) -> Option<Monitor> {
  monitors.iter().find(|m| m.focused && !m.disabled).cloned()
}

/// A change to one monitor's configuration. It lasts until the compositor
/// reloads its config.
#[derive(Debug, Clone, PartialEq)]
pub enum MonitorChange {
  Enable,
  Disable,
  /// Show the monitor of this name on it
  Mirror(String),
  StopMirroring,
  /// Mode, scale and position in one rule; `None` keeps what is set
  Set {
    mode: Option<Mode>,
    scale: Option<f32>,
    position: Option<(i32, i32)>,
  },
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
pub struct Window {
  pub address: String,
  pub monitor: i32,
  pub workspace: String,
  /// On a special workspace (scratchpad)
  pub special: bool,
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
  /// The client's process, 0 when unknown
  pub pid: i32,
}

impl Window {
  /// Draw order: special workspaces > pinned > fullscreen > floating > focus history
  pub fn stacking(&self) -> impl Ord + use<> {
    (
      self.special,
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
  use super::{Hardware, Mode, Monitor, Window, Workspace, logical_size};

  fn window(workspace: &str, floating: bool, focus_history_id: i32) -> Window {
    Window {
      address: String::new(),
      monitor: 0,
      workspace: workspace.into(),
      special: workspace.starts_with("special:"),
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
      pid: 0,
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

  #[test]
  fn logical_size_follows_scale_and_rotation() {
    assert_eq!(logical_size((1920, 1200), 1., 0), (1920, 1200));
    assert_eq!(logical_size((1920, 1200), 1.25, 0), (1536, 960));
    assert_eq!(logical_size((1920, 1200), 1.2, 0), (1600, 1000));
    // 90° and 270° (and flipped) turn it sideways, 180° does not
    assert_eq!(logical_size((1920, 1200), 1.2, 1), (1000, 1600));
    assert_eq!(logical_size((1920, 1200), 1.2, 2), (1600, 1000));
    assert_eq!(logical_size((1920, 1200), 1.2, 7), (1000, 1600));
    // a scale of zero is no scale
    assert_eq!(logical_size((100, 50), 0., 0), (100, 50));
  }

  #[test]
  fn parses_modes() {
    let m = Mode::parse("1920x1200@59.95Hz").unwrap();
    assert_eq!((m.width, m.height, m.millihertz()), (1920, 1200, 59950));
    assert!(Mode::parse("preferred").is_none());
    assert!(Mode::parse("axb@60Hz").is_none());
  }

  #[test]
  fn dpi_needs_a_physical_size() {
    let hw = Hardware {
      physical_mm: (508, 286),
      ..Default::default()
    };
    let mut m = Monitor {
      id: 0,
      name: "A".into(),
      width: 2560,
      height: 1440,
      refresh_rate: 60.,
      x: 0,
      y: 0,
      active_scratchpad: None,
      active_workspace: Workspace {
        id: "1".into(),
        name: "1".into(),
        monitor: "A".into(),
        monitor_id: 0,
      },
      scale: 1.,
      focused: false,
      disabled: false,
      mirror_of: "none".into(),
      reserved: [0; 4],
      hardware: hw,
    };
    assert_eq!(m.dpi().map(|d| d.round()), Some(128.));
    m.hardware.physical_mm = (0, 0);
    assert_eq!(m.dpi(), None);
  }
}
