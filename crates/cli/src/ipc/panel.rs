use clap::Subcommand;
use clap_complete::{ArgValueCandidates, CompletionCandidate};
use corona_ipc::IpcCommandSend;
use corona_surface::commands::{ClosePanel, ListPanels, OpenPanel, TogglePanel};

use crate::ipc::candidates;

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
  candidates(ListPanels::send(()))
}

impl PanelCommands {
  pub fn execute(self) {
    match self {
      PanelCommands::Open { panel } => {
        if let Err(e) = OpenPanel::send(panel) {
          tracing::error!("Failed to open panel: {}", e);
        }
      }
      PanelCommands::Close { panel } => {
        if let Err(e) = ClosePanel::send(panel) {
          tracing::error!("Failed to close panel: {}", e);
        }
      }
      PanelCommands::Toggle { panel } => {
        if let Err(e) = TogglePanel::send(panel) {
          tracing::error!("Failed to toggle panel: {}", e);
        }
      }
    }
  }
}
