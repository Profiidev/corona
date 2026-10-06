use clap::Subcommand;
use clap_complete::{ArgValueCandidates, CompletionCandidate};
use corona_ipc::IpcCommandSend;
use corona_shell::commands::power_profile::{CycleProfile, ListProfiles, SetProfile};

use crate::ipc::candidates;

#[derive(Subcommand)]
pub enum PowerProfileCommands {
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

impl PowerProfileCommands {
  pub fn execute(self) {
    let result = match self {
      PowerProfileCommands::Set { name } => SetProfile::send(name),
      PowerProfileCommands::Cycle => CycleProfile::send(()),
    };
    if let Err(e) = result {
      tracing::error!("Failed to change power profile: {}", e);
    }
  }
}
