use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::CompleteEnv;

use crate::{
  config::ConfigCommands, ipc::IpcCommands, network::NetworkCommands, schema::SchemaCommands,
  types::TypesCommand,
};

mod config;
mod ipc;
mod network;
mod schema;
mod types;

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
  /// Write the TypeScript declarations of plugins, as loading them does
  Types(TypesCommand),
  /// Network setup the shell needs root for
  #[command(hide = true)]
  Network {
    #[command(subcommand)]
    command: NetworkCommands,
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
    assert!(matches!(
      parse(&["types", "a", "b"]).unwrap().command,
      Commands::Types(_)
    ));
    assert!(matches!(
      parse(&[
        "network",
        "add-ap-interface",
        "0",
        "corona-ap",
        "02:00:00:00:00:01"
      ])
      .unwrap()
      .command,
      Commands::Network { .. }
    ));
    assert!(parse(&["network", "add-ap-interface", "phy0", "corona-ap", "x"]).is_err());
    assert!(parse(&["types"]).is_err());
    assert!(parse(&["types", "-o", "x.d.ts"]).is_err());
    assert!(parse(&[]).is_err());
    assert!(parse(&["nope"]).is_err());
  }
}
