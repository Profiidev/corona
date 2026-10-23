use clap::Subcommand;
use corona_ipc::IpcCommandSend;
use corona_shell::commands::notification::{ClearHistory, DismissToasts, Dnd, DoNotDisturb, Show};

#[derive(Subcommand)]
pub enum NotificationCommands {
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

impl NotificationCommands {
  pub fn execute(self) {
    match self {
      NotificationCommands::Dnd { action } => match DoNotDisturb::send(action) {
        Ok(enabled) if matches!(action, Dnd::Status) => {
          println!("{}", if enabled { "on" } else { "off" })
        }
        Ok(_) => {}
        Err(e) => tracing::error!("Failed to set do not disturb: {}", e),
      },
      NotificationCommands::Show { summary } => {
        if let Err(e) = Show::send(summary) {
          tracing::error!("Failed to show notification: {}", e);
        }
      }
      NotificationCommands::Dismiss => {
        if let Err(e) = DismissToasts::send(()) {
          tracing::error!("Failed to dismiss notifications: {}", e);
        }
      }
      NotificationCommands::ClearHistory => {
        if let Err(e) = ClearHistory::send(()) {
          tracing::error!("Failed to clear notification history: {}", e);
        }
      }
    }
  }
}
