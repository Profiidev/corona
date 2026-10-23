use clap::Subcommand;
use corona_ipc::IpcCommandSend;
use corona_shell::commands::clipboard::{ClearClipboard, DeleteClip, ListClipboard};

use crate::ipc::done;

#[derive(Subcommand)]
pub enum ClipboardCommands {
  /// Print the history, newest first: id, type and content
  List {
    /// Search the text
    query: Option<String>,
    #[arg(long, default_value_t = 50)]
    limit: usize,
  },
  /// Delete an entry by id
  Delete { id: i64 },
  /// Delete everything but the pinned entries
  Clear,
}

impl ClipboardCommands {
  pub fn execute(self) {
    match self {
      ClipboardCommands::List { query, limit } => match ListClipboard::send((query, limit)) {
        Ok(clips) => {
          for c in clips {
            let pin = if c.pinned { "*" } else { " " };
            let line = c.content.lines().next().unwrap_or_default();
            println!("{}{pin}\t{}\t{line}", c.id, c.kind);
          }
        }
        Err(e) => tracing::error!("Failed to list the clipboard: {}", e),
      },
      ClipboardCommands::Delete { id } => done(DeleteClip::send(id), "delete the entry"),
      ClipboardCommands::Clear => match ClearClipboard::send(()) {
        Ok(n) => println!("{n}"),
        Err(e) => tracing::error!("Failed to clear the clipboard: {}", e),
      },
    }
  }
}
