//! `corona ipc media`

use clap::Subcommand;
use clap_complete::{ArgValueCandidates, CompletionCandidate};
use corona_ipc::{
  IpcCommandSend, Switch,
  cli::{done, plain, print_lines, print_switch},
};

use crate::commands::{
  media::{Action, Media},
  player::{ListPlayers, Loop, LoopMode, Seek, SetPlayer, Shuffle, parse_seek},
};

#[derive(Subcommand)]
pub enum Commands {
  /// Go to the previous item
  Previous,
  /// Go to the next item
  Next,
  /// Toggle play/pause
  Toggle,
  /// Resume playback
  Play,
  /// Pause, no-op when nothing is playing
  Pause,
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
    #[arg(add = ArgValueCandidates::new(players))]
    player: String,
  },
  /// List the players
  List,
}

fn players() -> Vec<CompletionCandidate> {
  plain(ListPlayers::send(()).unwrap_or_default())
}

impl Commands {
  pub fn run(self) {
    match self {
      Commands::Previous => media(Action::Previous),
      Commands::Next => media(Action::Next),
      Commands::Toggle => media(Action::Toggle),
      Commands::Play => media(Action::Play),
      Commands::Pause => media(Action::Pause),
      Commands::Seek { position } => match parse_seek(&position) {
        Some(to) => done(Seek::send(to), "seek"),
        None => tracing::error!("expected seconds like 90, +10 or -10"),
      },
      Commands::Shuffle { switch } => print_switch(Shuffle::send(switch), switch, "shuffle"),
      Commands::Loop { mode } => done(Loop::send(mode), "set the loop mode"),
      Commands::Set { player } => done(SetPlayer::send(player), "set the player"),
      Commands::List => match ListPlayers::send(()) {
        Ok(players) => print_lines(players),
        Err(e) => tracing::error!("Failed to list players: {}", e),
      },
    }
  }
}

fn media(action: Action) {
  if let Err(e) = Media::send(action) {
    tracing::error!("Failed to run media action: {}", e);
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::commands::player::SeekTo;

  #[test]
  fn seeks() {
    assert_eq!(parse_seek("90"), Some(SeekTo::At(90.)));
    assert_eq!(parse_seek("+10"), Some(SeekTo::By(10.)));
    assert_eq!(parse_seek("-5.5"), Some(SeekTo::By(-5.5)));
    assert_eq!(parse_seek("soon"), None);
    for bad in ["nan", "inf", "-inf", "+NaN"] {
      assert_eq!(parse_seek(bad), None, "{bad}");
    }
  }
}
