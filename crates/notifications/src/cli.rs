//! `corona ipc notification`

use clap::Subcommand;
use corona_ipc::IpcCommandSend;

use crate::commands::{ClearHistory, DismissToasts, Dnd, DoNotDisturb, Show};

#[derive(Subcommand)]
pub enum Commands {
  /// Do not disturb: on, off, toggle or status
  Dnd { action: Dnd },
  /// Send a notification
  Show {
    /// The notification's summary
    summary: String,
  },
  /// Remove all entries from the notification history
  ClearHistory,
  /// Hide the notifications on screen, the history keeps them
  Dismiss,
}

fn fail(message: String) {
  tracing::error!("{message}");
  std::process::exit(1);
}

impl Commands {
  pub fn run(self) {
    match self {
      Commands::Dnd { action } => match DoNotDisturb::send(action) {
        Ok(enabled) if matches!(action, Dnd::Status) => {
          println!("{}", if enabled { "on" } else { "off" })
        }
        Ok(_) => {}
        Err(e) => fail(format!("Failed to set do not disturb: {e}")),
      },
      Commands::Show { summary } => {
        if let Err(e) = Show::send(summary) {
          fail(format!("Failed to show notification: {e}"));
        }
      }
      Commands::Dismiss => {
        if let Err(e) = DismissToasts::send(()) {
          fail(format!("Failed to dismiss notifications: {e}"));
        }
      }
      Commands::ClearHistory => {
        if let Err(e) = ClearHistory::send(()) {
          fail(format!("Failed to clear notification history: {e}"));
        }
      }
    }
  }
}
