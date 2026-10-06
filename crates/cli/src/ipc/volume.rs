use clap::Subcommand;
use corona_ipc::IpcCommandSend;
use corona_shell::commands::volume::{Action as VolumeAction, Device, Volume};

use crate::ipc::{STEP, level};

#[derive(Subcommand)]
pub enum VolumeCommands {
  /// Set the volume: 65, 65% or 0.65
  Set {
    #[arg(value_parser = level)]
    level: f32,
  },
  /// Raise the volume
  Up {
    /// The step: 10, 10% or 0.1
    #[arg(value_parser = level, default_value = STEP)]
    step: f32,
  },
  /// Lower the volume
  Down {
    /// The step: 10, 10% or 0.1
    #[arg(value_parser = level, default_value = STEP)]
    step: f32,
  },
  /// Toggle mute
  Mute,
}

impl VolumeCommands {
  pub fn execute(self, device: Device) {
    let action = match self {
      VolumeCommands::Set { level } => VolumeAction::Set(level),
      VolumeCommands::Up { step } => VolumeAction::Change(step),
      VolumeCommands::Down { step } => VolumeAction::Change(-step),
      VolumeCommands::Mute => VolumeAction::ToggleMute,
    };
    if let Err(e) = Volume::send((device, action)) {
      tracing::error!("Failed to change volume: {}", e);
    }
  }
}
