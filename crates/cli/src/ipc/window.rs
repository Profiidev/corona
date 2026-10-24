use crate::ipc::complete;
use clap::Subcommand;
use clap_complete::ArgValueCandidates;
use corona_ipc::IpcCommandSend;
use corona_shell::commands::monitor::{Change, Configure};
use corona_shell::commands::window::{
  CloseAll, FocusMonitor, KeyboardLayout, Layout, MoveRequest, MoveToMonitor, MoveToWorkspace,
  Place, ShowDesktop, Toggle, WindowToggle,
};

use crate::ipc::{done, print_lines};

#[derive(Subcommand)]
pub enum WindowCommands {
  /// Toggle floating, fullscreen or pin
  Toggle {
    what: WindowToggle,
    /// The window's address, the focused one by default
    #[arg(long, add = ArgValueCandidates::new(complete::windows))]
    address: Option<String>,
  },
  /// Move a window to a workspace
  MoveToWorkspace {
    #[arg(add = ArgValueCandidates::new(complete::workspaces))]
    workspace: String,
    /// Go along to the workspace
    #[arg(long)]
    follow: bool,
    #[arg(long, add = ArgValueCandidates::new(complete::windows))]
    address: Option<String>,
  },
  /// Move a window to the next, previous or a named monitor
  MoveToMonitor {
    #[arg(add = ArgValueCandidates::new(complete::monitor_targets))]
    monitor: String,
    #[arg(long, add = ArgValueCandidates::new(complete::windows))]
    address: Option<String>,
  },
  /// Float a window into a part of its monitor, or restore it
  Layout {
    #[arg(ignore_case = true)]
    layout: Layout,
    #[arg(long, add = ArgValueCandidates::new(complete::windows))]
    address: Option<String>,
  },
  /// Close every window but the excluded classes
  CloseAll {
    /// Leave the focused window open
    #[arg(long)]
    keep_focused: bool,
  },
}

impl WindowCommands {
  pub fn execute(self) {
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
    }
  }
}

#[derive(Subcommand)]
pub enum MonitorCommands {
  /// Focus the next, previous or a named monitor
  Focus {
    #[arg(add = ArgValueCandidates::new(complete::monitor_targets))]
    monitor: String,
  },
  /// Enable, disable or mirror a monitor until Hyprland reloads its config
  Configure {
    #[arg(add = ArgValueCandidates::new(complete::monitors))]
    monitor: String,
    #[arg(ignore_case = true)]
    change: Change,
    /// The monitor to show when mirroring
    #[arg(add = ArgValueCandidates::new(complete::monitors))]
    source: Option<String>,
  },
}

impl MonitorCommands {
  pub fn execute(self) {
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
    }
  }
}

pub fn show_desktop() {
  done(ShowDesktop::send(()), "show the desktop");
}

/// Lists the layouts, or switches to `next`, `previous`, an index or a name
pub fn keyboard_layout(which: Option<String>) {
  let listing = which.is_none();
  match KeyboardLayout::send(which) {
    Ok(layouts) if listing => print_lines(layouts),
    Ok(_) => {}
    Err(e) => tracing::error!("Failed to switch the keyboard layout: {}", e),
  }
}
