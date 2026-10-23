use clap::Subcommand;
use corona_ipc::IpcCommandSend;
use corona_shell::commands::{
  audio::{Kind, ListNodes, MoveStream, MuteFocused, SetDefault, StreamVolume},
  volume::Action,
};

use crate::ipc::{done, level};

#[derive(Subcommand)]
pub enum AudioCommands {
  /// List outputs, inputs or app streams
  List { kind: Kind },
  /// Make an output or input the default, by id, name or description
  Default { kind: Kind, device: String },
  /// Set an app stream's volume, by id or app name
  Stream {
    stream: String,
    #[arg(value_parser = level)]
    level: f32,
  },
  /// Mute or unmute an app stream
  MuteStream { stream: String },
  /// Play an app stream on another output
  Move { stream: String, output: String },
  /// Mute or unmute the focused window's audio
  MuteFocused,
}

impl AudioCommands {
  pub fn execute(self) {
    match self {
      AudioCommands::List { kind } => match ListNodes::send(kind) {
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
      AudioCommands::Default { kind, device } => {
        done(SetDefault::send((kind, device)), "set the default device")
      }
      AudioCommands::Stream { stream, level } => done(
        StreamVolume::send((stream, Action::Set(level))),
        "set the stream volume",
      ),
      AudioCommands::MuteStream { stream } => done(
        StreamVolume::send((stream, Action::ToggleMute)),
        "mute the stream",
      ),
      AudioCommands::Move { stream, output } => {
        done(MoveStream::send((stream, output)), "move the stream")
      }
      AudioCommands::MuteFocused => match MuteFocused::send(()) {
        Ok(muted) => println!("{}", if muted { "muted" } else { "unmuted" }),
        Err(e) => tracing::error!("Failed to mute the focused app: {}", e),
      },
    }
  }
}
