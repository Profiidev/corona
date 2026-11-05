//! `corona ipc power-profile`

use clap::Subcommand;
use clap_complete::{ArgValueCandidates, CompletionCandidate};
use corona_ipc::{IpcCommandSend, cli::candidates};

use crate::commands::{CycleProfile, ListProfiles, SetProfile};

#[derive(Subcommand)]
pub enum Commands {
  /// Activate a profile by name
  Set {
    /// The profile, like performance, balanced or power-saver
    #[arg(add = ArgValueCandidates::new(profile_names))]
    name: String,
  },
  /// Switch to the next profile, wrapping around
  Cycle,
}

/// Asks the running shell for the profiles UPower offers.
fn profile_names() -> Vec<CompletionCandidate> {
  candidates(ListProfiles::send(()))
}

impl Commands {
  pub fn run(self) {
    let result = match self {
      Commands::Set { name } => SetProfile::send(name),
      Commands::Cycle => CycleProfile::send(()),
    };
    if let Err(e) = result {
      tracing::error!("Failed to change power profile: {}", e);
    }
  }
}
