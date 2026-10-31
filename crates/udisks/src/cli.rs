//! `corona ipc drive`

use clap::Subcommand;
use clap_complete::{ArgValueCandidates, CompletionCandidate};
use corona_ipc::{
  IpcCommandSend,
  cli::{described, done},
};

use crate::commands::{Drive, DriveAction, DrivePasswordPrompt, ListDrives};

#[derive(Subcommand)]
pub enum Commands {
  /// List drives that are not the system's
  List,
  /// Mount a drive by device or label
  Mount {
    #[arg(add = ArgValueCandidates::new(drives))]
    drive: String,
  },
  /// Unmount a drive by device or label
  Unmount {
    #[arg(add = ArgValueCandidates::new(drives))]
    drive: String,
  },
  /// Unmount, lock and power off a drive by device or label
  Eject {
    #[arg(add = ArgValueCandidates::new(drives))]
    drive: String,
  },
  /// Ask for a drive's passphrase in a dialog and print it, exits 1 when cancelled. For
  /// udiskie's password_prompt: ["corona", "ipc", "drive", "password-prompt", "--device",
  /// "{device_presentation}"]
  PasswordPrompt {
    /// The device as shown in the dialog
    #[arg(long)]
    device: String,
  },
}

impl Commands {
  pub fn run(self) {
    match self {
      Commands::List => match ListDrives::send(()) {
        Ok(drives) => {
          for d in drives {
            println!("{}\t{}\t{}", d.device, d.label, d.mount_points.join(","));
          }
        }
        Err(e) => tracing::error!("Failed to list drives: {}", e),
      },
      Commands::Mount { drive } => done(DriveAction::send((Drive::Mount, drive)), "mount"),
      Commands::Unmount { drive } => done(DriveAction::send((Drive::Unmount, drive)), "unmount"),
      Commands::Eject { drive } => done(DriveAction::send((Drive::Eject, drive)), "eject"),
      Commands::PasswordPrompt { device } => match DrivePasswordPrompt::send(device) {
        Ok(Some(password)) => println!("{password}"),
        Ok(None) => std::process::exit(1),
        Err(e) => {
          tracing::error!("Failed to ask for the passphrase: {}", e);
          std::process::exit(1);
        }
      },
    }
  }
}

fn drives() -> Vec<CompletionCandidate> {
  described(
    ListDrives::send(())
      .unwrap_or_default()
      .into_iter()
      .map(|d| (d.device, d.label)),
  )
}
