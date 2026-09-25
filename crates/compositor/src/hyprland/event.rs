use std::{
  io::{BufRead, BufReader},
  os::unix::net::UnixStream,
  path::PathBuf,
  thread,
};

use anyhow::{Context, Result};
use gpui_kit::{App, BorrowAppContext};
use tracing::{debug, warn};

use crate::{
  Compositor,
  hyprland::{Hyprland, command::Ipc},
  types,
};

enum CompositorEvent {
  Workspace(Vec<types::Workspace>),
  ActiveWorkspace(types::Workspace),
  Monitor(Vec<types::Monitor>),
  ActiveMonitor(types::Monitor),
  Window(Vec<types::Window>),
  ActiveWindow(Option<types::Window>),
}

impl Hyprland {
  pub fn spawn_event_listener(cx: &mut App, ipc: Ipc, event_path: PathBuf) {
    let (tx, rx) = flume::bounded(100);

    thread::spawn(move || {
      loop {
        let socket = match UnixStream::connect(&event_path) {
          Ok(socket) => socket,
          Err(e) => {
            warn!("Failed to hyprland connect to event socket: {}", e);
            thread::sleep(std::time::Duration::from_secs(1));
            continue;
          }
        };

        for line in BufReader::new(socket).lines().map_while(Result::ok) {
          let events = match ipc.parse_event(&line) {
            Ok(events) => events,
            Err(e) => {
              warn!("Failed to parse hyprland event: {}", e);
              continue;
            }
          };

          for event in events {
            if tx.send(event).is_err() {
              break; // Channel closed, exit the loop
            }
          }
        }
      }
    });

    cx.spawn(async move |cx| {
      while let Ok(event) = rx.recv_async().await {
        cx.update(|cx| {
          cx.update_global::<Compositor, _>(|compositor, cx| match event {
            CompositorEvent::Workspace(workspaces) => compositor.workspaces.write(cx, workspaces),
            CompositorEvent::ActiveWorkspace(workspace) => {
              compositor.active_workspace.write(cx, workspace)
            }
            CompositorEvent::Monitor(monitors) => compositor.monitors.write(cx, monitors),
            CompositorEvent::ActiveMonitor(monitor) => compositor.active_monitor.write(cx, monitor),
            CompositorEvent::Window(windows) => compositor.windows.write(cx, windows),
            CompositorEvent::ActiveWindow(window) => compositor.active_window.write(cx, window),
          });
        });
      }
    })
    .detach();
  }
}

impl Ipc {
  fn parse_event(&self, event: &str) -> Result<Vec<CompositorEvent>> {
    let (name, data) = event
      .split_once(">>")
      .context("Invalid hyprland event format")?;

    let events = match name {
      "workspace" | "createworkspace" | "destroyworkspace" | "renameworkspace"
      | "moveworkspace" => {
        let workspaces = self.list_workspaces()?;
        let active = self.active_workspace()?;
        vec![
          CompositorEvent::Workspace(workspaces),
          CompositorEvent::ActiveWorkspace(active),
        ]
      }
      "openwindow" | "closewindow" | "movewindow" | "kill" | "windowtitle" => {
        let windows = self.list_windows()?;
        let window = self.active_window()?;

        vec![
          CompositorEvent::Window(windows),
          CompositorEvent::ActiveWindow(window),
        ]
      }
      "activewindow" => {
        let window = self.active_window()?;
        vec![CompositorEvent::ActiveWindow(window)]
      }
      "focusedmon" => {
        let (monitor_name, _) = data
          .split_once(",")
          .context("Invalid hyprland event format")?;

        let monitors = self.list_monitors()?;
        let monitor = monitors
          .into_iter()
          .find(|m| m.name == monitor_name)
          .context("Monitor not found")?;

        vec![
          CompositorEvent::ActiveWorkspace(monitor.active_workspace.clone()),
          CompositorEvent::ActiveMonitor(monitor),
        ]
      }
      "monitorremoved" | "monitoradded" => {
        let monitors = self.list_monitors()?;
        vec![CompositorEvent::Monitor(monitors)]
      }
      _ => {
        debug!(
          "Hyprland event parsing is not implemented yet for: {}",
          event
        );

        vec![]
      }
    };

    Ok(events)
  }
}
