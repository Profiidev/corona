use std::{
  io::{BufRead, BufReader},
  os::unix::net::UnixStream,
  path::PathBuf,
  thread,
};

use anyhow::{Context, Result};
use gpui_kit::{App, AppContext, Entity};
use tracing::{debug, warn};

use crate::integration::compositor::{
  event::{CompositorEvent, CompositorEventEmitter},
  hyprland::{Hyprland, command::Ipc},
};

impl Hyprland {
  pub fn spawn_event_listener(
    cx: &mut App,
    ipc: Ipc,
    event_path: PathBuf,
  ) -> Entity<CompositorEventEmitter> {
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
          if tx.send(line).is_err() {
            break; // Channel closed, exit the loop
          }
        }
      }
    });

    cx.new(|cx| {
      cx.spawn(async move |this, cx| {
        while let Ok(line) = rx.recv_async().await {
          let ipc = ipc.clone();
          let parsed = cx
            .background_spawn(async move { ipc.parse_event(&line) })
            .await;

          match parsed {
            Ok(events) => {
              for event in events {
                let _ = this.update(cx, |_, cx| cx.emit(event));
              }
            }
            Err(e) => warn!("Failed to parse hyprland event: {}", e),
          }
        }
      })
      .detach();

      CompositorEventEmitter
    })
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
          CompositorEvent::ActiveScratchpad(monitor.clone()),
          CompositorEvent::ActiveMonitor(monitor),
        ]
      }
      "monitorremoved" | "monitoradded" => {
        let monitors = self.list_monitors()?;
        vec![CompositorEvent::Monitor(monitors)]
      }
      "activespecial" => {
        let (_, monitor_name) = data
          .split_once(">>")
          .context("Invalid hyprland event format")?;

        let monitors = self.list_monitors()?;
        let monitor = monitors
          .into_iter()
          .find(|m| m.name == monitor_name)
          .context("Monitor not found")?;

        vec![CompositorEvent::ActiveScratchpad(monitor)]
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
