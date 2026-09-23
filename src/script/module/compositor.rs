use gpui_kit::{App, Entity};
use gpui_shell::{HostArguments, HostModule, HostObject, HostResult, HostValue, with_current_app};

use crate::{
  integration::compositor::{Compositor, CompositorExt, types},
  script::module::{Subscribe, Subscriptions, watch},
};

const DECLARATIONS: &str = r#"
export interface Workspace {
  id: string;
  name: string;
  monitor: string;
  monitor_id: number;
}

export interface Monitor {
  id: number;
  name: string;
  width: number;
  height: number;
  refresh_rate: number;
  x: number;
  y: number;
  active_scratchpad?: Workspace;
  active_workspace: Workspace;
  scale: number;
  focused: boolean;
  disabled: boolean;
  mirror_of: string;
}

export interface Window {
  address: string;
  monitor: number;
  workspace: string;
  class: string;
  title: string;
  x: number;
  y: number;
}

export interface Error {
  message: string;
}

export function listWorkspaces(): Workspace[];
export function activeWorkspace(): Workspace;

export function listMonitors(): Monitor[];
export function activeMonitor(): Monitor;

export function listWindows(): Window[];
export function activeWindow(): Window | null;

export function focusWorkspace(workspace: string): Error | null;
"#;

impl From<types::Workspace> for HostValue {
  fn from(value: types::Workspace) -> Self {
    HostObject::new()
      .field("id", value.id)
      .field("name", value.name)
      .field("monitor", value.monitor)
      .field("monitor_id", value.monitor_id)
      .into()
  }
}

impl From<types::Monitor> for HostValue {
  fn from(value: types::Monitor) -> Self {
    let mut obj = HostObject::new()
      .field("id", value.id)
      .field("name", value.name)
      .field("width", value.width)
      .field("height", value.height)
      .field("refresh_rate", value.refresh_rate)
      .field("x", value.x)
      .field("y", value.y)
      .field("active_workspace", HostValue::from(value.active_workspace))
      .field("scale", value.scale)
      .field("focused", value.focused)
      .field("disabled", value.disabled)
      .field("mirror_of", value.mirror_of);

    if let Some(active_scratchpad) = value.active_scratchpad {
      obj = obj.field("active_scratchpad", HostValue::from(active_scratchpad));
    };

    obj.into()
  }
}

impl From<types::Window> for HostValue {
  fn from(value: types::Window) -> Self {
    HostObject::new()
      .field("address", value.address)
      .field("monitor", value.monitor)
      .field("workspace", value.workspace)
      .field("class", value.class)
      .field("title", value.title)
      .field("x", value.x)
      .field("y", value.y)
      .into()
  }
}

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

fn read<W: 'static, R: Into<HostValue>>(
  reads: &Subscriptions,
  subs: &mut Vec<Subscribe>,
  update: Updates,
  entity: Entity<W>,
  read: impl Fn(&Compositor, &App) -> R + 'static,
) -> impl Fn(&HostArguments) -> HostResult + 'static {
  let sub = watch(reads, update.into(), entity.clone());
  subs.push(sub);
  let reads = reads.clone();

  move |_| {
    reads.record(update.into());
    Ok(with_current_app(|cx| read(cx.compositor(), cx).into()).unwrap_or(HostValue::Null))
  }
}

fn run(body: impl FnOnce(&mut gpui_kit::App) -> HostValue) -> HostValue {
  with_current_app(body).unwrap_or(HostValue::Null)
}

pub fn module(reads: &Subscriptions, subs: &mut Vec<Subscribe>, cx: &mut App) -> HostModule {
  HostModule::new("corona/compositor")
    .declarations(DECLARATIONS)
    .function(
      "listWorkspaces",
      read(
        reads,
        subs,
        Updates::Workspaces,
        cx.compositor().workspaces.clone(),
        |compositor, cx| compositor.list_workspaces(cx).to_vec(),
      ),
    )
    .function(
      "activeWorkspace",
      read(
        reads,
        subs,
        Updates::ActiveWorkspace,
        cx.compositor().active_workspace.clone(),
        |compositor, cx| compositor.active_workspace(cx).clone(),
      ),
    )
    .function(
      "listMonitors",
      read(
        reads,
        subs,
        Updates::Monitors,
        cx.compositor().monitors.clone(),
        |compositor, cx| compositor.list_monitors(cx).to_vec(),
      ),
    )
    .function(
      "activeMonitor",
      read(
        reads,
        subs,
        Updates::ActiveMonitor,
        cx.compositor().active_monitor.clone(),
        |compositor, cx| compositor.active_monitor(cx).clone(),
      ),
    )
    .function(
      "listWindows",
      read(
        reads,
        subs,
        Updates::Windows,
        cx.compositor().windows.clone(),
        |compositor, cx| compositor.list_windows(cx).to_vec(),
      ),
    )
    .function(
      "activeWindow",
      read(
        reads,
        subs,
        Updates::ActiveWindow,
        cx.compositor().active_window.clone(),
        |compositor, cx| compositor.active_window(cx).cloned(),
      ),
    )
    .function("focusWorkspace", move |args| {
      let workspace = args.string(0)?.to_owned();

      Ok(run(|cx| {
        match cx.compositor().focus_workspace(&workspace) {
          Ok(()) => HostValue::Null,
          Err(error) => HostObject::new().field("message", error.to_string()).into(),
        }
      }))
    })
}
