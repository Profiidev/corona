//! `corona ipc audio`

use clap::Subcommand;
use clap_complete::{ArgValueCandidates, CompletionCandidate};
use corona_ipc::{
  IpcCommandSend,
  cli::{STEP, described, done, level},
};

use crate::commands::{
  audio::{Kind, ListNodes, MoveStream, MuteFocused, SetDefault, StreamVolume},
  volume::{Action, Device, Volume},
};

#[derive(Subcommand)]
pub enum Commands {
  /// List outputs, inputs or app streams
  List { kind: Kind },
  /// Make an output or input the default, by id, name or description
  Default {
    kind: Kind,
    #[arg(add = ArgValueCandidates::new(devices))]
    device: String,
  },
  /// Set an app stream's volume, by id or app name
  Stream {
    #[arg(add = ArgValueCandidates::new(streams))]
    stream: String,
    #[arg(value_parser = level)]
    level: f32,
  },
  /// Mute or unmute an app stream
  MuteStream {
    #[arg(add = ArgValueCandidates::new(streams))]
    stream: String,
  },
  /// Play an app stream on another output
  Move {
    #[arg(add = ArgValueCandidates::new(streams))]
    stream: String,
    #[arg(add = ArgValueCandidates::new(outputs))]
    output: String,
  },
  /// Mute or unmute the focused window's audio
  MuteFocused,
  /// Output volume commands
  Volume {
    #[command(subcommand)]
    command: VolumeCommands,
  },
  /// Microphone volume commands
  Mic {
    #[command(subcommand)]
    command: VolumeCommands,
  },
}

impl Commands {
  pub fn run(self) {
    match self {
      Commands::List { kind } => match ListNodes::send(kind) {
        Ok(nodes) => {
          for n in nodes {
            let default = if n.default { "*" } else { " " };
            let mute = if n.mute { " muted" } else { "" };
            println!(
              "{default} {:>4} {:>3}%{mute} {}",
              n.id,
              (n.volume * 100.).round(),
              n.description
            );
          }
        }
        Err(e) => tracing::error!("Failed to list audio devices: {}", e),
      },
      Commands::Default { kind, device } => {
        done(SetDefault::send((kind, device)), "set the default device")
      }
      Commands::Stream { stream, level } => done(
        StreamVolume::send((stream, Action::Set(level))),
        "set the stream volume",
      ),
      Commands::MuteStream { stream } => done(
        StreamVolume::send((stream, Action::ToggleMute)),
        "mute the stream",
      ),
      Commands::Move { stream, output } => {
        done(MoveStream::send((stream, output)), "move the stream")
      }
      Commands::MuteFocused => match MuteFocused::send(()) {
        Ok(muted) => println!("{}", if muted { "muted" } else { "unmuted" }),
        Err(e) => tracing::error!("Failed to mute the focused app: {}", e),
      },
      Commands::Volume { command } => command.run(Device::Output),
      Commands::Mic { command } => command.run(Device::Input),
    }
  }
}

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
  pub fn run(self, device: Device) {
    let action = match self {
      VolumeCommands::Set { level } => Action::Set(level),
      VolumeCommands::Up { step } => Action::Change(step),
      VolumeCommands::Down { step } => Action::Change(-step),
      VolumeCommands::Mute => Action::ToggleMute,
    };
    if let Err(e) = Volume::send((device, action)) {
      tracing::error!("Failed to change volume: {}", e);
    }
  }
}

fn nodes(kind: Kind) -> Vec<CompletionCandidate> {
  described(
    ListNodes::send(kind)
      .unwrap_or_default()
      .into_iter()
      .map(|n| (n.name, n.description)),
  )
}

fn devices() -> Vec<CompletionCandidate> {
  let mut all = nodes(Kind::Outputs);
  all.extend(nodes(Kind::Inputs));
  all
}

fn outputs() -> Vec<CompletionCandidate> {
  nodes(Kind::Outputs)
}

fn streams() -> Vec<CompletionCandidate> {
  described(
    ListNodes::send(Kind::Streams)
      .unwrap_or_default()
      .into_iter()
      .map(|n| (n.id.to_string(), n.description)),
  )
}
