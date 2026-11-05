//! `corona ipc panel`, `theme` and `control-center`

use clap::Subcommand;
use clap_complete::{ArgValueCandidates, CompletionCandidate};
use corona_ipc::{IpcCommandSend, cli::candidates};

use crate::{
  commands::{ClosePanel, ListPages, ListPanels, OpenPage, OpenPanel, TogglePanel},
  theme::{ListThemes, Mode, SetMode, SetTheme},
};

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
  pub fn run(self) {
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

#[derive(Subcommand)]
pub enum ThemeCommands {
  /// Dark, light, toggle or get the mode
  Mode { mode: Mode },
  /// Switch to a theme by name
  Set {
    /// The name of the theme
    #[arg(add = ArgValueCandidates::new(theme_names))]
    name: String,
  },
}

/// Asks the running shell, which knows every theme it loaded.
fn theme_names() -> Vec<CompletionCandidate> {
  candidates(ListThemes::send(()))
}

impl ThemeCommands {
  pub fn run(self) {
    match self {
      ThemeCommands::Mode { mode } => match SetMode::send(mode) {
        Ok(current) if matches!(mode, Mode::Get) => println!("{current}"),
        Ok(_) => {}
        Err(e) => tracing::error!("Failed to set theme mode: {}", e),
      },
      ThemeCommands::Set { name } => {
        if let Err(e) = SetTheme::send(name) {
          tracing::error!("Failed to set theme: {}", e);
        }
      }
    }
  }
}

#[derive(clap::Args)]
pub struct ControlCenterArgs {
  #[arg(add = ArgValueCandidates::new(|| candidates(ListPages::send(()))))]
  page: String,
}

impl ControlCenterArgs {
  pub fn run(self) {
    if let Err(e) = OpenPage::send(self.page) {
      tracing::error!("Failed to open the control center: {}", e);
    }
  }
}
