use std::{
  io::{BufRead, BufReader},
  os::unix::net::UnixStream,
  path::PathBuf,
  thread,
};

use anyhow::{Context, Result};
use gpui_kit::{App, AppContext, Entity};
use tracing::{debug, warn};

use crate::compositor::{
  event::{CompositorEvent, CompositorEventEmitter},
  hyprland::{Hyprland, command::Ipc},
};

impl Hyprland {
  pub fn spawn_event_listener(
    cx: &mut App,
    ipc: Ipc,
    event_path: PathBuf,
  ) -> Entity<CompositorEventEmitter> {
    let (tx, rx) = async_channel::bounded(100);

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
          if tx.send_blocking(line).is_err() {
            break; // Channel closed, exit the loop
          }
        }
      }
    });

    cx.new(|cx| {
      cx.spawn(async move |this, cx| {
        while let Ok(line) = rx.recv().await {
          match ipc.parse_event(&line) {
            Ok(Some(event)) => {
              let _ = this.update(cx, |_, cx| cx.emit(event));
            }
            Err(e) => warn!("Failed to parse hyprland event: {}", e),
            _ => (),
          }
        }
      })
      .detach();

      CompositorEventEmitter
    })
  }
}

impl Ipc {
  fn parse_event(&self, event: &str) -> Result<Option<CompositorEvent>> {
    let (name, _data) = event
      .split_once(">>")
      .context("Invalid hyprland event format")?;

    let event = match name {
      "workspace" => {
        let workspaces = self.get_workspaces()?;
        CompositorEvent::WorkspaceChanged(workspaces)
      }
      _ => {
        debug!(
          "Hyprland event parsing is not implemented yet for: {}",
          event
        );

        return Ok(None);
      }
    };

    Ok(Some(event))
  }
}
