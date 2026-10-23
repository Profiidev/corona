use clap::Subcommand;
use corona_ipc::IpcCommandSend;
use corona_shell::commands::tray::{Activate, ListEntries};

use crate::ipc::done;

#[derive(Subcommand)]
pub enum TrayCommands {
  /// List tray items and their menu entries
  List,
  /// Activate an item, or click a menu entry by id
  Activate {
    address: String,
    #[arg(long)]
    menu: Option<i32>,
  },
}

impl TrayCommands {
  pub fn execute(self) {
    match self {
      TrayCommands::List => match ListEntries::send(()) {
        Ok(entries) => {
          for e in entries {
            let menu = e.menu.map(|m| m.to_string()).unwrap_or_default();
            println!("{}\t{menu}\t{}", e.address, e.label);
          }
        }
        Err(e) => tracing::error!("Failed to list the tray: {}", e),
      },
      TrayCommands::Activate { address, menu } => {
        done(Activate::send((address, menu)), "activate the tray item")
      }
    }
  }
}
