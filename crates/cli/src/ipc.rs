use clap::Subcommand;
use clap_complete::{ArgValueCandidates, CompletionCandidate};
use corona_ipc::IpcCommandSend;
use corona_shell::media::{Action as MediaAction, Media};
use corona_shell::notification::{ClearHistory, Dnd, DoNotDisturb, Show};
use corona_shell::overlays::{
  colorpicker::commands::ColorPicker,
  screenshot::{commands::Screenshot, mode::Mode},
  switcher::commands::{Cycle, Mode as SwitcherMode, Modifier, Options},
};
use corona_shell::session::{Action, Session};

#[derive(Subcommand)]
pub enum IpcCommands {
  /// Panel commands
  Panel {
    #[command(subcommand)]
    command: PanelCommands,
  },
  /// Open the screenshot overlay
  Screenshot {
    /// selection, monitor or window
    #[arg(default_value = "selection", ignore_case = true)]
    mode: Mode,
  },
  /// Pick a color from the screen and copy its hex code
  ColorPicker,
  /// Control the active MPRIS player
  Media { action: MediaAction },
  /// Notification commands
  Notification {
    #[command(subcommand)]
    command: NotificationCommands,
  },
  /// Lock, suspend, log out, reboot or shut down
  Session { action: Action },
  /// Open the window switcher, or move its selection while open
  Switcher {
    /// window or workspace
    #[arg(long, default_value = "window", ignore_case = true)]
    mode: SwitcherMode,
    /// Held to keep the switcher open, releasing it switches: super, alt or ctrl
    #[arg(long, default_value = "super", ignore_case = true)]
    modifier: Modifier,
    /// Only show what is on the focused monitor
    #[arg(long)]
    current_monitor: bool,
  },
}

impl IpcCommands {
  pub fn execute(self) {
    match self {
      IpcCommands::Panel { command } => command.execute(),
      IpcCommands::Screenshot { mode } => {
        if let Err(e) = Screenshot::send(mode) {
          tracing::error!("Failed to start screenshot: {}", e);
        }
      }
      IpcCommands::ColorPicker => {
        if let Err(e) = ColorPicker::send(()) {
          tracing::error!("Failed to start color picker: {}", e);
        }
      }
      IpcCommands::Media { action } => {
        if let Err(e) = Media::send(action) {
          tracing::error!("Failed to run media action: {}", e);
        }
      }
      IpcCommands::Notification { command } => command.execute(),
      IpcCommands::Session { action } => {
        if let Err(e) = Session::send(action) {
          tracing::error!("Failed to run session action: {}", e);
        }
      }
      IpcCommands::Switcher {
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
pub enum PanelCommands {
  /// Open a panel
  Open {
    /// The name of the panel to open
    #[arg(add = ArgValueCandidates::new(panel_names))]
    panel: String,
  },
  /// Close a panel
  Close {
    /// The name of the panel to close
    #[arg(add = ArgValueCandidates::new(panel_names))]
    panel: String,
  },
  /// Toggle a panel
  Toggle {
    /// The name of the panel to toggle
    #[arg(add = ArgValueCandidates::new(panel_names))]
    panel: String,
  },
}

fn panel_names() -> Vec<CompletionCandidate> {
  corona_surface::commands::ListPanels::send(())
    .unwrap_or_default()
    .into_iter()
    .map(CompletionCandidate::new)
    .collect()
}

impl PanelCommands {
  pub fn execute(self) {
    match self {
      PanelCommands::Open { panel } => {
        if let Err(e) = corona_surface::commands::OpenPanel::send(panel) {
          tracing::error!("Failed to open panel: {}", e);
        }
      }
      PanelCommands::Close { panel } => {
        if let Err(e) = corona_surface::commands::ClosePanel::send(panel) {
          tracing::error!("Failed to close panel: {}", e);
        }
      }
      PanelCommands::Toggle { panel } => {
        if let Err(e) = corona_surface::commands::TogglePanel::send(panel) {
          tracing::error!("Failed to toggle panel: {}", e);
        }
      }
    }
  }
}

#[derive(Subcommand)]
pub enum NotificationCommands {
  /// Do not disturb: on, off, toggle or status
  Dnd { action: Dnd },
  /// Send a notification
  Show {
    /// The notification's summary
    summary: String,
  },
  /// Remove all entries from the notification history
  ClearHistory,
}

impl NotificationCommands {
  pub fn execute(self) {
    match self {
      NotificationCommands::Dnd { action } => match DoNotDisturb::send(action) {
        Ok(enabled) if matches!(action, Dnd::Status) => {
          println!("{}", if enabled { "on" } else { "off" })
        }
        Ok(_) => {}
        Err(e) => tracing::error!("Failed to set do not disturb: {}", e),
      },
      NotificationCommands::Show { summary } => {
        if let Err(e) = Show::send(summary) {
          tracing::error!("Failed to show notification: {}", e);
        }
      }
      NotificationCommands::ClearHistory => {
        if let Err(e) = ClearHistory::send(()) {
          tracing::error!("Failed to clear notification history: {}", e);
        }
      }
    }
  }
}
