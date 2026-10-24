use crate::ipc::complete;
use clap::Subcommand;
use clap_complete::ArgValueCandidates;
use corona_ipc::IpcCommandSend;
use corona_shell::commands::{
  player::{ListPlayers, Loop, LoopMode, Seek, SeekTo, SetPlayer, Shuffle},
  radio::Switch,
};

use crate::ipc::{done, print_lines, print_switch};

#[derive(Subcommand)]
pub enum PlayerCommands {
  /// Seek to seconds, or by `+10` / `-10`
  Seek {
    #[arg(allow_hyphen_values = true)]
    position: String,
  },
  /// Shuffle: on, off, toggle or status
  Shuffle { switch: Switch },
  /// Loop: none, track or playlist
  Loop {
    #[arg(ignore_case = true)]
    mode: LoopMode,
  },
  /// Make a player active, by name like Spotify
  Set {
    #[arg(add = ArgValueCandidates::new(complete::players))]
    player: String,
  },
  /// List the players
  List,
}

fn seek_to(position: &str) -> Option<SeekTo> {
  let relative = position.starts_with(['+', '-']);
  let seconds: f64 = position.parse().ok()?;
  Some(match relative {
    true => SeekTo::By(seconds),
    false => SeekTo::At(seconds),
  })
}

impl PlayerCommands {
  pub fn execute(self) {
    match self {
      PlayerCommands::Seek { position } => match seek_to(&position) {
        Some(to) => done(Seek::send(to), "seek"),
        None => tracing::error!("expected seconds like 90, +10 or -10"),
      },
      PlayerCommands::Shuffle { switch } => print_switch(Shuffle::send(switch), switch, "shuffle"),
      PlayerCommands::Loop { mode } => done(Loop::send(mode), "set the loop mode"),
      PlayerCommands::Set { player } => done(SetPlayer::send(player), "set the player"),
      PlayerCommands::List => match ListPlayers::send(()) {
        Ok(players) => print_lines(players),
        Err(e) => tracing::error!("Failed to list players: {}", e),
      },
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn seeks() {
    assert_eq!(seek_to("90"), Some(SeekTo::At(90.)));
    assert_eq!(seek_to("+10"), Some(SeekTo::By(10.)));
    assert_eq!(seek_to("-5.5"), Some(SeekTo::By(-5.5)));
    assert_eq!(seek_to("soon"), None);
  }
}
