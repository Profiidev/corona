//! `corona ipc session`

use corona_ipc::IpcCommandSend;

use corona_shell::ipc::auth::{Action, Session};

#[derive(clap::Args)]
pub struct SessionArgs {
  action: Action,
}

impl SessionArgs {
  pub fn run(self) {
    if let Err(e) = Session::send(self.action) {
      tracing::error!("Failed to run session action: {}", e);
    }
  }
}
