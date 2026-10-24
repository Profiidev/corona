use clap::Subcommand;
use clap_complete::ArgValueCandidates;

use crate::ipc::complete;
use corona_ipc::IpcCommandSend;
use corona_shell::launcher::commands::{Close, Open, OpenRequest, Toggle};

#[derive(Subcommand)]
pub enum LauncherCommands {
  /// Open the launcher, optionally inside a command or with a query
  Open {
    /// Id of the command to open straight into, like `volume`
    #[arg(long, add = ArgValueCandidates::new(complete::commands))]
    command: Option<String>,
    /// Text to search for, or the command's arguments
    #[arg(long)]
    query: Option<String>,
  },
  Close,
  Toggle,
}

impl LauncherCommands {
  pub fn execute(self) {
    let result = match self {
      LauncherCommands::Open { command, query } => Open::send(OpenRequest { command, query }),
      LauncherCommands::Close => Close::send(()),
      LauncherCommands::Toggle => Toggle::send(()),
    };
    if let Err(e) = result {
      tracing::error!("Failed to control the launcher: {}", e);
    }
  }
}
