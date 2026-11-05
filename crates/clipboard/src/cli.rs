//! `corona ipc clipboard`

use clap::Subcommand;
use clap_complete::{ArgValueCandidates, CompletionCandidate};
use corona_ipc::{
  IpcCommandSend,
  cli::{described, done},
};

use crate::commands::{ClearClipboard, DeleteClip, ListClipboard};

#[derive(Subcommand)]
pub enum Commands {
  /// Print the history, newest first: id, type and content
  List {
    /// Search the text
    query: Option<String>,
    #[arg(long, default_value_t = 50)]
    limit: usize,
  },
  /// Delete an entry by id
  Delete {
    #[arg(add = ArgValueCandidates::new(clips))]
    id: i64,
  },
  /// Delete everything but the pinned entries
  Clear,
}

impl Commands {
  pub fn run(self) {
    match self {
      Commands::List { query, limit } => match ListClipboard::send((query, limit)) {
        Ok(clips) => {
          for c in clips {
            let pin = if c.pinned { "*" } else { " " };
            let line = first_line(&c.content);
            println!("{}{pin}\t{}\t{line}", c.id, c.kind);
          }
        }
        Err(e) => tracing::error!("Failed to list the clipboard: {}", e),
      },
      Commands::Delete { id } => done(DeleteClip::send(id), "delete the entry"),
      Commands::Clear => match ClearClipboard::send(()) {
        Ok(n) => println!("{n}"),
        Err(e) => tracing::error!("Failed to clear the clipboard: {}", e),
      },
    }
  }
}

fn clips() -> Vec<CompletionCandidate> {
  described(
    ListClipboard::send((None, 50))
      .unwrap_or_default()
      .into_iter()
      .map(|c| (c.id.to_string(), first_line(&c.content))),
  )
}

/// The first line with control characters replaced, the text is untrusted and the terminal
/// would act on escape sequences
fn first_line(text: &str) -> String {
  let line = text.lines().next().unwrap_or_default();
  line
    .chars()
    .map(|c| if c.is_control() { ' ' } else { c })
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn first_line_sanitises() {
    assert_eq!(first_line("a\x1b[31mred\nsecond"), "a [31mred");
    assert_eq!(first_line("\nsecond"), "");
  }
}
