use clap::Subcommand;
use corona_ipc::IpcCommandSend;
use corona_shell::overlays::screenshot::{commands::Screenshot, mode::Mode};

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
    #[arg(default_value = "selection")]
    mode: Mode,
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
    }
  }
}

#[derive(Subcommand)]
pub enum PanelCommands {
  /// Open a panel
  Open {
    /// The name of the panel to open
    panel: String,
  },
  /// Close a panel
  Close {
    /// The name of the panel to close
    panel: String,
  },
  /// Toggle a panel
  Toggle {
    /// The name of the panel to toggle
    panel: String,
  },
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
