use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::CompleteEnv;

use crate::ipc::IpcCommands;

mod ipc;

/// Corona Shell CLI
#[derive(Parser)]
#[command(name = "corona")]
pub struct Cli {
  #[command(subcommand)]
  pub command: Commands,
}

impl Cli {
  pub fn parse() -> Self {
    CompleteEnv::with_factory(Cli::command).complete();
    <Cli as Parser>::parse()
  }
}

#[derive(Subcommand)]
pub enum Commands {
  /// IPC commands
  Ipc {
    #[command(subcommand)]
    command: IpcCommands,
  },
  /// Run corona shell
  Shell,
}
