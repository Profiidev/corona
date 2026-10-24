use std::collections::HashMap;

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use corona_compositor::{Compositor, CompositorExt, types};
use corona_config::ConfigProvider;
use corona_ipc::{IpcCommand, IpcServer};
use gpui_kit::{App, Global};
use serde::{Deserialize, Serialize};

pub fn register_commands(server: &mut IpcServer) {
  server
    .register::<Toggle>()
    .register::<MoveToWorkspace>()
    .register::<MoveToMonitor>()
    .register::<Place>()
    .register::<CloseAll>()
    .register::<FocusMonitor>()
    .register::<ShowDesktop>()
    .register::<KeyboardLayout>()
    .register::<ListWindows>()
    .register::<ListWorkspaces>()
    .register::<ListMonitors>();
}

/// The window at `address`, or the focused one
fn target(address: Option<String>, cx: &mut App) -> Result<types::Window> {
  Compositor::refresh_windows(cx)?;
  let compositor = cx.compositor();
  match address {
    Some(address) => compositor
      .list_windows(cx)
      .iter()
      .find(|w| w.address == address)
      .cloned()
      .with_context(|| format!("no window {address}")),
    None => compositor
      .active_window(cx)
      .cloned()
      .context("no window is focused"),
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
pub enum WindowToggle {
  Floating,
  Fullscreen,
  Pin,
}

/// Toggles floating, fullscreen or pinned of a window, the focused one by default
pub struct Toggle;

impl IpcCommand for Toggle {
  const COMMAND: &'static str = "window:toggle";

  type Payload = (WindowToggle, Option<String>);
  type Response = ();

  fn handle((toggle, address): Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let window = target(address, cx)?;
    let compositor = cx.compositor();
    match toggle {
      WindowToggle::Floating => compositor.toggle_floating(&window.address),
      WindowToggle::Fullscreen => compositor.toggle_fullscreen(&window.address),
      WindowToggle::Pin => compositor.toggle_pin(&window.address),
    }
  }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveRequest {
  pub address: Option<String>,
  pub workspace: String,
  /// Go along to the workspace
  pub follow: bool,
}

pub struct MoveToWorkspace;

impl IpcCommand for MoveToWorkspace {
  const COMMAND: &'static str = "window:move_workspace";

  type Payload = MoveRequest;
  type Response = ();

  fn handle(request: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let window = target(request.address, cx)?;
    cx.compositor()
      .move_to_workspace(&window.address, &request.workspace, request.follow)
  }
}

/// `next`, `previous` or a monitor's name
pub struct MoveToMonitor;

impl IpcCommand for MoveToMonitor {
  const COMMAND: &'static str = "window:move_monitor";

  type Payload = (String, Option<String>);
  type Response = ();

  fn handle((monitor, address): Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let window = target(address, cx)?;
    let monitors: Vec<types::Monitor> = cx
      .compositor()
      .list_monitors(cx)
      .iter()
      .filter(|m| !m.disabled)
      .cloned()
      .collect();
    let name = neighbour(&monitors, window.monitor, &monitor)?;
    cx.compositor().move_to_monitor(&window.address, &name)
  }
}

/// The monitor `which` names relative to the one with id `current`
fn neighbour(monitors: &[types::Monitor], current: u32, which: &str) -> Result<String> {
  if monitors.is_empty() {
    bail!("no monitors");
  }
  let index = monitors.iter().position(|m| m.id == current).unwrap_or(0);
  let n = monitors.len();
  let pick = match which {
    "next" => (index + 1) % n,
    "previous" | "prev" => (index + n - 1) % n,
    name => {
      return monitors
        .iter()
        .find(|m| m.name == name)
        .map(|m| m.name.clone())
        .with_context(|| format!("no monitor {name}"));
    }
  };
  Ok(monitors[pick].name.clone())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
pub enum Layout {
  Left,
  Right,
  Top,
  Bottom,
  LeftThird,
  CenterThird,
  RightThird,
  LeftTwoThirds,
  RightTwoThirds,
  TopLeft,
  TopRight,
  BottomLeft,
  BottomRight,
  Center,
  Maximize,
  AlmostMaximize,
  Restore,
}

impl Layout {
  pub const ALL: [Layout; 17] = [
    Layout::Left,
    Layout::Right,
    Layout::Top,
    Layout::Bottom,
    Layout::LeftThird,
    Layout::CenterThird,
    Layout::RightThird,
    Layout::LeftTwoThirds,
    Layout::RightTwoThirds,
    Layout::TopLeft,
    Layout::TopRight,
    Layout::BottomLeft,
    Layout::BottomRight,
    Layout::Center,
    Layout::Maximize,
    Layout::AlmostMaximize,
    Layout::Restore,
  ];

  /// The window's rect on a monitor area `(x, y, w, h)`; None for restore
  pub fn rect(
    self,
    area: (i32, i32, i32, i32),
    window: (i32, i32),
  ) -> Option<(i32, i32, i32, i32)> {
    let (x, y, w, h) = area;
    let part = |num: i32, den: i32, v: i32| v * num / den;
    Some(match self {
      Layout::Left => (x, y, w / 2, h),
      Layout::Right => (x + w / 2, y, w - w / 2, h),
      Layout::Top => (x, y, w, h / 2),
      Layout::Bottom => (x, y + h / 2, w, h - h / 2),
      Layout::LeftThird => (x, y, w / 3, h),
      Layout::CenterThird => (x + w / 3, y, w / 3, h),
      Layout::RightThird => (x + part(2, 3, w), y, w - part(2, 3, w), h),
      Layout::LeftTwoThirds => (x, y, part(2, 3, w), h),
      Layout::RightTwoThirds => (x + w / 3, y, w - w / 3, h),
      Layout::TopLeft => (x, y, w / 2, h / 2),
      Layout::TopRight => (x + w / 2, y, w - w / 2, h / 2),
      Layout::BottomLeft => (x, y + h / 2, w / 2, h - h / 2),
      Layout::BottomRight => (x + w / 2, y + h / 2, w - w / 2, h - h / 2),
      Layout::Center => {
        let (ww, wh) = (window.0.min(w), window.1.min(h));
        (x + (w - ww) / 2, y + (h - wh) / 2, ww, wh)
      }
      Layout::Maximize => (x, y, w, h),
      Layout::AlmostMaximize => {
        let (mx, my) = (w / 20, h / 20);
        (x + mx, y + my, w - 2 * mx, h - 2 * my)
      }
      Layout::Restore => return None,
    })
  }
}

/// Where windows were before a layout moved them, for restore
#[derive(Default)]
struct Placed(HashMap<String, (Rect, bool)>);

type Rect = (i32, i32, i32, i32);

impl Global for Placed {}

/// Floats the window into a part of its monitor, or back where it was
pub struct Place;

impl IpcCommand for Place {
  const COMMAND: &'static str = "window:layout";

  type Payload = (Layout, Option<String>);
  type Response = ();

  fn handle((layout, address): Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let window = target(address, cx)?;
    let monitor = cx
      .compositor()
      .list_monitors(cx)
      .iter()
      .find(|m| m.id == window.monitor)
      .cloned()
      .context("the window's monitor is gone")?;
    let scale = monitor.scale.max(0.1);
    let area = (
      monitor.x,
      monitor.y,
      (monitor.width as f32 / scale) as i32,
      (monitor.height as f32 / scale) as i32,
    );
    let current = (window.x, window.y, window.width, window.height);
    let Some(rect) = layout.rect(area, (window.width, window.height)) else {
      let placed = cx.default_global::<Placed>().0.remove(&window.address);
      let Some((rect, floating)) = placed else {
        bail!("nothing to restore");
      };
      cx.compositor().place_window(&window.address, rect)?;
      if !floating {
        cx.compositor().toggle_floating(&window.address)?;
      }
      return Ok(());
    };
    cx.default_global::<Placed>()
      .0
      .entry(window.address.clone())
      .or_insert((current, window.floating));
    cx.compositor().place_window(&window.address, rect)
  }
}

/// Closes every window but those of excluded classes, and the focused one when asked
pub struct CloseAll;

impl IpcCommand for CloseAll {
  const COMMAND: &'static str = "window:close_all";

  /// Keep the focused window
  type Payload = bool;
  /// How many were closed
  type Response = usize;

  fn handle(keep_focused: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    Compositor::refresh_windows(cx)?;
    let exclude = cx.config().launcher.close_all_exclude.clone();
    let compositor = cx.compositor();
    let focused = compositor.active_window(cx).map(|w| w.address.clone());
    let windows: Vec<String> = compositor
      .list_windows(cx)
      .iter()
      .filter(|w| !exclude.iter().any(|c| c.eq_ignore_ascii_case(&w.class)))
      .filter(|w| !(keep_focused && Some(&w.address) == focused.as_ref()))
      .map(|w| w.address.clone())
      .collect();
    for address in &windows {
      compositor.close_window(address)?;
    }
    Ok(windows.len())
  }
}

pub struct FocusMonitor;

impl IpcCommand for FocusMonitor {
  const COMMAND: &'static str = "monitor:focus";

  /// A name, `next` or `previous`
  type Payload = String;
  type Response = ();

  fn handle(which: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let compositor = cx.compositor();
    let monitors: Vec<types::Monitor> = compositor
      .list_monitors(cx)
      .iter()
      .filter(|m| !m.disabled)
      .cloned()
      .collect();
    let current = compositor.active_monitor(cx).id;
    let name = neighbour(&monitors, current, &which)?;
    compositor.focus_monitor(&name)
  }
}

/// The special workspace "Show desktop" toggles; it stays empty
pub const DESKTOP: &str = "corona-desktop";

pub struct ShowDesktop;

impl IpcCommand for ShowDesktop {
  const COMMAND: &'static str = "workspace:show_desktop";

  type Payload = ();
  type Response = ();

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    cx.compositor().toggle_special_workspace(DESKTOP)
  }
}

/// Lists the main keyboard's layouts, or switches to `next`, `previous`, an index or a name
pub struct KeyboardLayout;

impl IpcCommand for KeyboardLayout {
  const COMMAND: &'static str = "keyboard:layout";

  type Payload = Option<String>;
  /// The layouts; the active one first after a switch
  type Response = Vec<String>;

  fn handle(which: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let compositor = cx.compositor();
    let layouts = compositor.keyboard_layouts()?;
    let Some(which) = which else {
      return Ok(layouts);
    };
    let index = match which.as_str() {
      "next" | "previous" | "prev" => {
        let active = compositor
          .keyboard_layout(cx)
          .unwrap_or_default()
          .to_lowercase();
        let current = layouts
          .iter()
          .position(|l| active.starts_with(l) || active.contains(l.as_str()))
          .unwrap_or(0);
        let n = layouts.len().max(1);
        match which.as_str() {
          "next" => (current + 1) % n,
          _ => (current + n - 1) % n,
        }
      }
      other => match other.parse::<usize>() {
        Ok(index) => index,
        Err(_) => layouts
          .iter()
          .position(|l| l == other)
          .with_context(|| format!("no layout {other}, expected one of {}", layouts.join(", ")))?,
      },
    };
    compositor.set_keyboard_layout(index)?;
    let mut layouts = layouts;
    if index < layouts.len() {
      let active = layouts.remove(index);
      layouts.insert(0, active);
    }
    Ok(layouts)
  }
}

/// Window addresses with class and title
pub struct ListWindows;

impl IpcCommand for ListWindows {
  const COMMAND: &'static str = "window:list";

  type Payload = ();
  type Response = Vec<(String, String)>;

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    Compositor::refresh_windows(cx)?;
    Ok(
      cx.compositor()
        .list_windows(cx)
        .iter()
        .map(|w| (w.address.clone(), format!("{}: {}", w.class, w.title)))
        .collect(),
    )
  }
}

/// Workspace ids with their names
pub struct ListWorkspaces;

impl IpcCommand for ListWorkspaces {
  const COMMAND: &'static str = "workspace:list";

  type Payload = ();
  type Response = Vec<(String, String)>;

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    Ok(
      cx.compositor()
        .list_workspaces(cx)
        .iter()
        .map(|w| (w.id.clone(), w.name.clone()))
        .collect(),
    )
  }
}

/// Monitor names
pub struct ListMonitors;

impl IpcCommand for ListMonitors {
  const COMMAND: &'static str = "monitor:list";

  type Payload = ();
  type Response = Vec<String>;

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    Ok(
      cx.compositor()
        .list_monitors(cx)
        .iter()
        .map(|m| m.name.clone())
        .collect(),
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn layout_rects() {
    let area = (100, 0, 1920, 1080);
    assert_eq!(Layout::Left.rect(area, (0, 0)), Some((100, 0, 960, 1080)));
    assert_eq!(
      Layout::RightThird.rect(area, (0, 0)),
      Some((1380, 0, 640, 1080))
    );
    assert_eq!(
      Layout::BottomRight.rect(area, (0, 0)),
      Some((1060, 540, 960, 540))
    );
    assert_eq!(
      Layout::Center.rect(area, (800, 600)),
      Some((660, 240, 800, 600))
    );
    assert_eq!(
      Layout::AlmostMaximize.rect(area, (0, 0)),
      Some((196, 54, 1728, 972))
    );
    assert_eq!(Layout::Restore.rect(area, (0, 0)), None);
    // halves cover the area without a gap
    let (l, r) = (
      Layout::Left.rect((0, 0, 1001, 10), (0, 0)).unwrap(),
      Layout::Right.rect((0, 0, 1001, 10), (0, 0)).unwrap(),
    );
    assert_eq!(l.2 + r.2, 1001);
  }

  #[test]
  fn neighbours() {
    let monitor = |id, name: &str| types::Monitor {
      id,
      name: name.into(),
      width: 1,
      height: 1,
      refresh_rate: 60.,
      x: 0,
      y: 0,
      active_scratchpad: None,
      active_workspace: types::Workspace {
        id: "1".into(),
        name: "1".into(),
        monitor: name.into(),
        monitor_id: id,
      },
      scale: 1.,
      focused: false,
      disabled: false,
      mirror_of: String::new(),
    };
    let monitors = [monitor(0, "DP-1"), monitor(1, "HDMI-A-1")];
    assert_eq!(neighbour(&monitors, 0, "next").unwrap(), "HDMI-A-1");
    assert_eq!(neighbour(&monitors, 0, "previous").unwrap(), "HDMI-A-1");
    assert_eq!(neighbour(&monitors, 1, "DP-1").unwrap(), "DP-1");
    assert!(neighbour(&monitors, 1, "eDP-1").is_err());
  }
}
