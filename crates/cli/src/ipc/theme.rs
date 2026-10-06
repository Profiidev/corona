use clap::Subcommand;
use clap_complete::{ArgValueCandidates, CompletionCandidate};
use corona_ipc::IpcCommandSend;
use corona_shell::commands::theme::{ListThemes, Mode as ThemeMode, SetMode, SetTheme};

use crate::ipc::candidates;

#[derive(Subcommand)]
pub enum ThemeCommands {
  /// Dark, light, toggle or get the mode
  Mode { mode: ThemeMode },
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
  pub fn execute(self) {
    match self {
      ThemeCommands::Mode { mode } => match SetMode::send(mode) {
        Ok(current) if matches!(mode, ThemeMode::Get) => println!("{current}"),
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
