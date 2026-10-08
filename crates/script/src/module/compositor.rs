use corona_compositor::{Compositor, CompositorExt};
use gpui_kit::App;
use gpui_shell::HostModule;
use serde::Serialize;
use ts_rs::TS;

use crate::{
  host_fn::{Glob, Module},
  module::{Subscribe, Subscriptions, read},
};
use corona_macros::host_fn;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Updates {
  Workspaces,
  ActiveWorkspace,
  Monitors,
  ActiveMonitor,
  Windows,
  ActiveWindow,
}

impl From<Updates> for super::Updates {
  fn from(value: Updates) -> Self {
    super::Updates::Compositor(value)
  }
}

#[host_fn]
fn focus_workspace(compositor: Glob<Compositor>, workspace: String) -> anyhow::Result<()> {
  compositor.focus_workspace(&workspace)
}

#[host_fn]
fn focus_window(compositor: Glob<Compositor>, address: String) -> anyhow::Result<()> {
  compositor.focus_window(&address)
}

#[host_fn]
fn close_window(compositor: Glob<Compositor>, address: String) -> anyhow::Result<()> {
  compositor.close_window(&address)
}

/// In global layout coordinates.
#[derive(Serialize, TS)]
struct Position {
  x: i32,
  y: i32,
}

#[host_fn]
fn cursor_position(compositor: Glob<Compositor>) -> anyhow::Result<Position> {
  let (x, y) = compositor.cursor_position()?;
  Ok(Position { x, y })
}

pub fn module(reads: &Subscriptions, subs: &mut Vec<Subscribe>, cx: &mut App) -> HostModule {
  let compositor = cx.compositor();

  Module::new("corona/compositor")
    .func(read(
      reads,
      subs,
      "listWorkspaces",
      Updates::Workspaces,
      compositor.workspaces.clone(),
      |cx| cx.compositor().list_workspaces(cx).to_vec(),
    ))
    .func(read(
      reads,
      subs,
      "activeWorkspace",
      Updates::ActiveWorkspace,
      compositor.active_workspace.clone(),
      |cx| cx.compositor().active_workspace(cx).clone(),
    ))
    .func(read(
      reads,
      subs,
      "listMonitors",
      Updates::Monitors,
      compositor.monitors.clone(),
      |cx| cx.compositor().list_monitors(cx).to_vec(),
    ))
    .func(read(
      reads,
      subs,
      "activeMonitor",
      Updates::ActiveMonitor,
      compositor.active_monitor.clone(),
      |cx| cx.compositor().active_monitor(cx).clone(),
    ))
    .func(read(
      reads,
      subs,
      "listWindows",
      Updates::Windows,
      compositor.windows.clone(),
      |cx| cx.compositor().list_windows(cx).to_vec(),
    ))
    .func(read(
      reads,
      subs,
      "activeWindow",
      Updates::ActiveWindow,
      compositor.active_window.clone(),
      |cx| cx.compositor().active_window(cx).cloned(),
    ))
    .func(focus_workspace)
    .func(focus_window)
    .func(close_window)
    .func(cursor_position)
    .into()
}
