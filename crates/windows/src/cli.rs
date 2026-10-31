//! `corona ipc window`, `monitor` and `keyboard`

use clap::Subcommand;
use clap_complete::{ArgValueCandidates, CompletionCandidate};
use corona_ipc::{
  IpcCommandSend,
  cli::{described, done, plain, print_lines},
};

use crate::commands::{
  dpms::{Dpms, Power},
  lock_key::{LockKey, Pressed},
  monitor::{Change, Configure},
  switcher::{Cycle, Mode, Modifier, Options},
  window::{
    CloseAll, FocusMonitor, KeyboardLayout, Layout, ListMonitors, ListWindows, ListWorkspaces,
    MoveRequest, MoveToMonitor, MoveToWorkspace, Place, ShowDesktop, Toggle, WindowToggle,
  },
};

#[derive(Subcommand)]
pub enum WindowCommands {
  /// Toggle floating, fullscreen or pin
  Toggle {
    what: WindowToggle,
    /// The window's address, the focused one by default
    #[arg(long, add = ArgValueCandidates::new(windows))]
    address: Option<String>,
  },
  /// Move a window to a workspace
  MoveToWorkspace {
    #[arg(add = ArgValueCandidates::new(workspaces))]
    workspace: String,
    /// Go along to the workspace
    #[arg(long)]
    follow: bool,
    #[arg(long, add = ArgValueCandidates::new(windows))]
    address: Option<String>,
  },
  /// Move a window to the next, previous or a named monitor
  MoveToMonitor {
    #[arg(add = ArgValueCandidates::new(monitor_targets))]
    monitor: String,
    #[arg(long, add = ArgValueCandidates::new(windows))]
    address: Option<String>,
  },
  /// Float a window into a part of its monitor, or restore it
  Layout {
    #[arg(ignore_case = true)]
    layout: Layout,
    #[arg(long, add = ArgValueCandidates::new(windows))]
    address: Option<String>,
  },
  /// Close every window but the excluded classes
  CloseAll {
    /// Leave the focused window open
    #[arg(long)]
    keep_focused: bool,
  },
  /// Show the desktop, or the windows again
  ShowDesktop,
  /// Open the window switcher, or move its selection while open
  Switcher {
    /// window or workspace
    #[arg(long, default_value = "window", ignore_case = true)]
    mode: Mode,
    /// Held to keep the switcher open, releasing it switches: super, alt or ctrl
    #[arg(long, default_value = "super", ignore_case = true)]
    modifier: Modifier,
    /// Only show what is on the focused monitor
    #[arg(long)]
    current_monitor: bool,
  },
}

impl WindowCommands {
  pub fn run(self) {
    match self {
      WindowCommands::Toggle { what, address } => done(Toggle::send((what, address)), "toggle"),
      WindowCommands::MoveToWorkspace {
        workspace,
        follow,
        address,
      } => done(
        MoveToWorkspace::send(MoveRequest {
          address,
          workspace,
          follow,
        }),
        "move the window",
      ),
      WindowCommands::MoveToMonitor { monitor, address } => {
        done(MoveToMonitor::send((monitor, address)), "move the window")
      }
      WindowCommands::Layout { layout, address } => {
        done(Place::send((layout, address)), "place the window")
      }
      WindowCommands::CloseAll { keep_focused } => match CloseAll::send(keep_focused) {
        Ok(n) => println!("{n}"),
        Err(e) => tracing::error!("Failed to close the windows: {}", e),
      },
      WindowCommands::ShowDesktop => done(ShowDesktop::send(()), "show the desktop"),
      WindowCommands::Switcher {
        mode,
        modifier,
        current_monitor,
      } => {
        let options = Options {
          mode,
          modifier,
          current_monitor,
        };
        if let Err(e) = Cycle::send(options) {
          tracing::error!("Failed to cycle window switcher: {}", e);
        }
      }
    }
  }
}

#[derive(Subcommand)]
pub enum MonitorCommands {
  /// Focus the next, previous or a named monitor
  Focus {
    #[arg(add = ArgValueCandidates::new(monitor_targets))]
    monitor: String,
  },
  /// Enable, disable or mirror a monitor until Hyprland reloads its config
  Configure {
    #[arg(add = ArgValueCandidates::new(monitors))]
    monitor: String,
    #[arg(ignore_case = true)]
    change: Change,
    /// The monitor to show when mirroring
    #[arg(add = ArgValueCandidates::new(monitors))]
    source: Option<String>,
  },
  /// Turn every monitor on or off
  Dpms { power: Power },
}

impl MonitorCommands {
  pub fn run(self) {
    match self {
      MonitorCommands::Focus { monitor } => done(FocusMonitor::send(monitor), "focus the monitor"),
      MonitorCommands::Configure {
        monitor,
        change,
        source,
      } => done(
        Configure::send((monitor, change, source)),
        "configure the monitor",
      ),
      MonitorCommands::Dpms { power } => {
        if let Err(e) = Dpms::send(power) {
          tracing::error!("Failed to switch the monitors: {}", e);
        }
      }
    }
  }
}

#[derive(Subcommand)]
pub enum KeyboardCommands {
  /// List keyboard layouts, or switch to next, previous, an index or a name
  Layout {
    #[arg(add = ArgValueCandidates::new(layouts))]
    layout: Option<String>,
  },
  /// Show the OSD of a lock key: caps, num or scroll. Bind it to the key in
  /// Hyprland, e.g. `hl.bind("Caps_Lock", hl.dsp.exec_cmd("corona ipc keyboard lock-key caps"))`
  LockKey { key: LockKey },
}

impl KeyboardCommands {
  pub fn run(self) {
    match self {
      KeyboardCommands::Layout { layout } => {
        let listing = layout.is_none();
        match KeyboardLayout::send(layout) {
          Ok(layouts) if listing => print_lines(layouts),
          Ok(_) => {}
          Err(e) => tracing::error!("Failed to switch the keyboard layout: {}", e),
        }
      }
      KeyboardCommands::LockKey { key } => {
        if let Err(e) = Pressed::send(key) {
          tracing::error!("Failed to show the lock key: {}", e);
        }
      }
    }
  }
}

fn windows() -> Vec<CompletionCandidate> {
  described(ListWindows::send(()).unwrap_or_default())
}

fn workspaces() -> Vec<CompletionCandidate> {
  described(ListWorkspaces::send(()).unwrap_or_default())
}

fn monitors() -> Vec<CompletionCandidate> {
  plain(ListMonitors::send(()).unwrap_or_default())
}

/// `next`, `previous` and every monitor
fn monitor_targets() -> Vec<CompletionCandidate> {
  let mut all = plain(["next".to_string(), "previous".to_string()]);
  all.extend(monitors());
  all
}

/// `next`, `previous` and every layout
fn layouts() -> Vec<CompletionCandidate> {
  let mut all = plain(["next".to_string(), "previous".to_string()]);
  all.extend(plain(KeyboardLayout::send(None).unwrap_or_default()));
  all
}
