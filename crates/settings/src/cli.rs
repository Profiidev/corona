//! `corona ipc settings`

use clap_complete::ArgValueCandidates;
use corona_ipc::{IpcCommandSend, cli::candidates};

use crate::commands::{Action, ListPages, SettingsWindow};

#[derive(clap::Args)]
pub struct SettingsArgs {
  action: Action,
  /// The page to open on
  #[arg(add = ArgValueCandidates::new(|| candidates(ListPages::send(()))))]
  page: Option<String>,
}

impl SettingsArgs {
  pub fn run(self) {
    if let Err(e) = SettingsWindow::send((self.action, self.page)) {
      tracing::error!("settings command failed: {}", e);
    }
  }
}
