use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::CompleteEnv;

use crate::{config::ConfigCommands, ipc::IpcCommands, schema::SchemaCommands};

mod config;
mod ipc;
mod schema;

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
  /// Settings commands
  Config {
    #[command(subcommand)]
    command: ConfigCommands,
  },
  /// Print the JSON schemas of plugin files, for editors
  Schema {
    #[command(subcommand)]
    command: SchemaCommands,
  },
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn command_tree_is_valid() {
    Cli::command().debug_assert();
  }

  #[test]
  fn top_level() {
    let parse = |args: &[&str]| <Cli as Parser>::try_parse_from(["corona"].iter().chain(args));
    assert!(matches!(
      parse(&["shell"]).unwrap().command,
      Commands::Shell
    ));
    assert!(matches!(
      parse(&["config", "validate"]).unwrap().command,
      Commands::Config { .. }
    ));
    assert!(matches!(
      parse(&["schema", "plugin"]).unwrap().command,
      Commands::Schema { .. }
    ));
    assert!(parse(&["schema", "catalog", "-o", "x.json"]).is_ok());
    assert!(parse(&["schema", "other"]).is_err());
    assert!(parse(&[]).is_err());
    assert!(parse(&["nope"]).is_err());
  }
}
