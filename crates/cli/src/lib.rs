use clap::{Parser, Subcommand};

use crate::ipc::IpcCommands;

mod ipc;

/// Corona Shell CLI
#[derive(Parser)]
pub struct Cli {
  #[command(subcommand)]
  pub command: Commands,
}

impl Cli {
  pub fn parse() -> Self {
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
