use std::{
  io::{BufRead, BufReader},
  os::unix::net::UnixStream,
  path::PathBuf,
  thread,
};

use anyhow::{Context, Result};
use corona_utils::entity::WriteChangedExt;
use gpui_kit::{App, BorrowAppContext};
use tracing::{debug, warn};

use crate::{
  Compositor,
  hyprland::{Hyprland, command::Ipc},
  types,
};

#[derive(Debug, PartialEq)]
enum CompositorEvent {
  Workspace(Vec<types::Workspace>),
  ActiveWorkspace(types::Workspace),
  Monitor(Vec<types::Monitor>),
  ActiveMonitor(types::Monitor),
  Window(Vec<types::Window>),
  ActiveWindow(Option<types::Window>),
  KeyboardLayout(Option<String>),
  Urgent(String),
  Attended(String),
}

#[cfg(not(test))]
const RECONNECT: std::time::Duration = std::time::Duration::from_secs(1);
#[cfg(test)]
const RECONNECT: std::time::Duration = std::time::Duration::from_millis(20);

impl Hyprland {
  pub fn spawn_event_listener(cx: &mut App, ipc: Ipc, event_path: PathBuf) {
    let (tx, rx) = flume::bounded(100);

    thread::spawn(move || {
      // ends once nobody listens anymore
      'listen: loop {
        let socket = match UnixStream::connect(&event_path) {
          Ok(socket) => socket,
          Err(e) => {
            warn!("Failed to hyprland connect to event socket: {}", e);
            thread::sleep(RECONNECT);
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
              break 'listen;
            }
          }
        }
      }
    });

    cx.spawn(async move |cx| {
      while let Ok(event) = rx.recv_async().await {
        cx.update(|cx| {
          cx.update_global::<Compositor, _>(|compositor, cx| apply(compositor, event, cx))
        });
      }
    })
    .detach();
  }
}

fn apply(compositor: &Compositor, event: CompositorEvent, cx: &mut App) {
  match event {
    CompositorEvent::Workspace(workspaces) => compositor.workspaces.write_changed(cx, workspaces),
    CompositorEvent::ActiveWorkspace(workspace) => {
      compositor.active_workspace.write_changed(cx, workspace)
    }
    CompositorEvent::Monitor(monitors) => compositor.monitors.write_changed(cx, monitors),
    CompositorEvent::ActiveMonitor(monitor) => compositor.active_monitor.write_changed(cx, monitor),
    CompositorEvent::Window(windows) => compositor.windows.write_changed(cx, windows),
    CompositorEvent::ActiveWindow(window) => compositor.active_window.write_changed(cx, window),
    CompositorEvent::KeyboardLayout(layout) => compositor.keyboard_layout.write_changed(cx, layout),
    CompositorEvent::Urgent(address) => compositor.urgent.update(cx, |urgent, cx| {
      if urgent.insert(address) {
        cx.notify();
      }
    }),
    CompositorEvent::Attended(address) => compositor.urgent.update(cx, |urgent, cx| {
      if urgent.remove(&address) {
        cx.notify();
      }
    }),
  }
}

/// The window an event names, None when it names none
fn window_address(data: &str) -> Option<String> {
  let address = data.split(',').next().unwrap_or_default();
  let address = address.trim_start_matches("0x");
  (!address.is_empty()).then(|| format!("0x{address}"))
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
        let monitors = self.list_monitors()?;
        let mut events = vec![
          CompositorEvent::Workspace(workspaces),
          CompositorEvent::ActiveWorkspace(active),
          CompositorEvent::Monitor(monitors),
        ];
        // its windows move along, and no window event says so
        if name == "moveworkspace" {
          events.push(CompositorEvent::Window(self.list_windows()?));
        }
        events
      }
      "activespecial" => vec![CompositorEvent::Monitor(self.list_monitors()?)],
      // fullscreen, floating and pinned change how windows stack
      "openwindow" | "closewindow" | "movewindow" | "kill" | "windowtitle" | "fullscreen"
      | "changefloatingmode" | "pin" => {
        let windows = self.list_windows()?;
        let window = self.active_window()?;

        let mut events = vec![
          CompositorEvent::Window(windows),
          CompositorEvent::ActiveWindow(window),
        ];
        if name == "closewindow" {
          events.extend(window_address(data).map(CompositorEvent::Attended));
        }
        events
      }
      "urgent" => window_address(data)
        .map(CompositorEvent::Urgent)
        .into_iter()
        .collect(),
      // focus left every window when it names none
      "activewindowv2" => window_address(data)
        .map(CompositorEvent::Attended)
        .into_iter()
        .collect(),
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
      "activelayout" => vec![CompositorEvent::KeyboardLayout(self.keyboard_layout()?)],
      // a monitor that goes away, or starts mirroring another (which Hyprland
      // reports as removed and added), hands its workspaces and their windows
      // to the monitors left, without workspace or window events
      "monitorremoved" | "monitoradded" => vec![
        CompositorEvent::Monitor(self.list_monitors()?),
        CompositorEvent::Workspace(self.list_workspaces()?),
        CompositorEvent::ActiveWorkspace(self.active_workspace()?),
        CompositorEvent::Window(self.list_windows()?),
      ],
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

#[cfg(test)]
mod tests {
  use super::*;
  use crate::hyprland::fake::FakeHyprland;

  #[test]
  fn window_address() {
    let address = |data| super::window_address(data);
    assert_eq!(address("5ba3a8eef560").as_deref(), Some("0x5ba3a8eef560"));
    assert_eq!(address("0x5ba3a8eef560").as_deref(), Some("0x5ba3a8eef560"));
    assert_eq!(address("5ba3a8eef560,1").as_deref(), Some("0x5ba3a8eef560"));
    assert_eq!(
      address("abc,1,title, with commas").as_deref(),
      Some("0xabc")
    );
    assert_eq!(address("5BA3").as_deref(), Some("0x5BA3"));
    assert_eq!(address("0x5BA3").as_deref(), Some("0x5BA3"));
    assert_ne!(address("5ba3"), address("5BA3"));
    assert_eq!(address(""), None);
    assert_eq!(address(","), None);
    assert_eq!(address("0x"), None);
  }

  fn parse(hypr: &FakeHyprland, line: &str) -> Result<Vec<CompositorEvent>> {
    hypr.ipc().parse_event(line)
  }

  fn names(events: &[CompositorEvent]) -> Vec<&'static str> {
    events
      .iter()
      .map(|e| match e {
        CompositorEvent::Workspace(_) => "Workspace",
        CompositorEvent::ActiveWorkspace(_) => "ActiveWorkspace",
        CompositorEvent::Monitor(_) => "Monitor",
        CompositorEvent::ActiveMonitor(_) => "ActiveMonitor",
        CompositorEvent::Window(_) => "Window",
        CompositorEvent::ActiveWindow(_) => "ActiveWindow",
        CompositorEvent::KeyboardLayout(_) => "KeyboardLayout",
        CompositorEvent::Urgent(_) => "Urgent",
        CompositorEvent::Attended(_) => "Attended",
      })
      .collect()
  }

  #[test]
  fn workspace_events_refresh_workspaces_and_monitors() {
    let hypr = FakeHyprland::start();
    for name in [
      "workspace",
      "createworkspace",
      "destroyworkspace",
      "renameworkspace",
      "moveworkspace",
    ] {
      let events = parse(&hypr, &format!("{name}>>3")).unwrap();
      let mut want = vec!["Workspace", "ActiveWorkspace", "Monitor"];
      // a moved workspace takes its windows to another monitor
      if name == "moveworkspace" {
        want.push("Window");
      }
      assert_eq!(names(&events), want, "{name}");
    }
    assert_eq!(
      names(&parse(&hypr, "activespecial>>special:x,DP-1").unwrap()),
      ["Monitor"]
    );
    // mirroring is a monitor removed and added: workspaces and windows move
    for name in ["monitoradded", "monitorremoved"] {
      assert_eq!(
        names(&parse(&hypr, &format!("{name}>>DP-2")).unwrap()),
        ["Monitor", "Workspace", "ActiveWorkspace", "Window"],
        "{name}"
      );
    }
  }

  #[test]
  fn window_events() {
    let hypr = FakeHyprland::start();
    for name in ["openwindow", "movewindow", "kill", "windowtitle"] {
      let events = parse(&hypr, &format!("{name}>>a,1,x,y")).unwrap();
      assert_eq!(names(&events), ["Window", "ActiveWindow"], "{name}");
    }
    let closed = parse(&hypr, "closewindow>>abc").unwrap();
    assert_eq!(names(&closed), ["Window", "ActiveWindow", "Attended"]);
    assert_eq!(closed[2], CompositorEvent::Attended("0xabc".into()));
    let active = parse(&hypr, "activewindow>>firefox,title").unwrap();
    assert_eq!(names(&active), ["ActiveWindow"]);
  }

  #[test]
  fn kill_does_not_clear_urgency() {
    let hypr = FakeHyprland::start();
    // kill emits Window and ActiveWindow, but NOT Attended (leaving urgent addresses intact)
    let killed = parse(&hypr, "kill>>abc").unwrap();
    assert_eq!(names(&killed), ["Window", "ActiveWindow"]);
    assert!(
      !killed
        .iter()
        .any(|e| matches!(e, CompositorEvent::Attended(_)))
    );
  }

  #[test]
  fn activewindowv2_does_not_update_active_window() {
    let hypr = FakeHyprland::start();
    // activewindowv2 only attends urgency; it does not query or emit ActiveWindow
    let events = parse(&hypr, "activewindowv2>>abc").unwrap();
    assert_eq!(names(&events), ["Attended"]);
    assert!(
      !events
        .iter()
        .any(|e| matches!(e, CompositorEvent::ActiveWindow(_)))
    );
  }

  #[test]
  fn monitorremoved_does_not_refresh_active_monitor() {
    let hypr = FakeHyprland::start();
    let events = parse(&hypr, "monitorremoved>>DP-1").unwrap();
    assert_eq!(
      names(&events),
      ["Monitor", "Workspace", "ActiveWorkspace", "Window"]
    );
    assert!(
      !events
        .iter()
        .any(|e| matches!(e, CompositorEvent::ActiveMonitor(_)))
    );
  }

  #[test]
  fn urgency() {
    let hypr = FakeHyprland::start();
    assert_eq!(
      parse(&hypr, "urgent>>abc").unwrap(),
      [CompositorEvent::Urgent("0xabc".into())]
    );
    assert_eq!(
      parse(&hypr, "activewindowv2>>abc").unwrap(),
      [CompositorEvent::Attended("0xabc".into())]
    );
    // focus left every window
    assert!(parse(&hypr, "activewindowv2>>").unwrap().is_empty());
    assert!(parse(&hypr, "activewindowv2>>,").unwrap().is_empty());
    // no IPC needed for these
    assert!(hypr.commands().is_empty());
  }

  #[test]
  fn empty_addresses_are_ignored() {
    let hypr = FakeHyprland::start();
    assert!(parse(&hypr, "urgent>>").unwrap().is_empty());
    assert!(parse(&hypr, "urgent>>,").unwrap().is_empty());
    // a close without an address still refreshes the windows
    assert_eq!(
      names(&parse(&hypr, "closewindow>>").unwrap()),
      ["Window", "ActiveWindow"]
    );
  }

  #[test]
  fn window_state_changes_refresh_windows() {
    let hypr = FakeHyprland::start();
    for line in ["fullscreen>>1", "changefloatingmode>>abc,1", "pin>>abc,1"] {
      assert!(
        names(&parse(&hypr, line).unwrap()).contains(&"Window"),
        "{line}"
      );
    }
  }

  #[test]
  fn focused_monitor() {
    let hypr = FakeHyprland::start();
    let events = parse(&hypr, "focusedmon>>DP-1,2").unwrap();
    let [
      CompositorEvent::ActiveWorkspace(workspace),
      CompositorEvent::ActiveMonitor(monitor),
    ] = &events[..]
    else {
      panic!("{events:?}");
    };
    assert_eq!(
      (workspace.name.as_str(), monitor.name.as_str()),
      ("2", "DP-1")
    );
    assert_eq!(
      parse(&hypr, "focusedmon>>DP-1").unwrap_err().to_string(),
      "Invalid hyprland event format"
    );
    assert_eq!(
      parse(&hypr, "focusedmon>>HDMI-A-1,1")
        .unwrap_err()
        .to_string(),
      "Monitor not found"
    );
  }

  #[test]
  fn keyboard_layout() {
    let hypr = FakeHyprland::start();
    assert_eq!(
      parse(&hypr, "activelayout>>at-translated-set-2-keyboard,German").unwrap(),
      [CompositorEvent::KeyboardLayout(Some("German".into()))]
    );
  }

  #[test]
  fn malformed_and_unknown_lines() {
    let hypr = FakeHyprland::start();
    assert_eq!(
      parse(&hypr, "garbage").unwrap_err().to_string(),
      "Invalid hyprland event format"
    );
    assert!(parse(&hypr, "").is_err());
    assert!(parse(&hypr, "configreloaded>>").unwrap().is_empty());
    assert!(parse(&hypr, "submap>>resize").unwrap().is_empty());
    // the first `>>` splits, data may contain more
    assert_eq!(
      parse(&hypr, "urgent>>a>>b").unwrap(),
      [CompositorEvent::Urgent("0xa>>b".into())]
    );
    // an IPC failure fails the event
    hypr.answer("j/clients", "not json");
    assert!(parse(&hypr, "openwindow>>a").is_err());
  }
}
