//! `corona ipc tray`

use clap::Subcommand;
use clap_complete::{ArgValueCandidates, CompletionCandidate};
use corona_ipc::{
  IpcCommandSend,
  cli::{described, done},
};

use crate::commands::{Activate, ListEntries};

#[derive(Subcommand)]
pub enum Commands {
  /// List tray items and their menu entries
  List,
  /// Activate an item, or click a menu entry by id
  Activate {
    #[arg(add = ArgValueCandidates::new(items))]
    address: String,
    #[arg(long, add = ArgValueCandidates::new(menus))]
    menu: Option<i32>,
  },
}

impl Commands {
  pub fn run(self) {
    match self {
      Commands::List => match ListEntries::send(()) {
        Ok(entries) => {
          for e in entries {
            let menu = e.menu.map(|m| m.to_string()).unwrap_or_default();
            println!("{}\t{menu}\t{}", e.address, e.label);
          }
        }
        Err(e) => tracing::error!("Failed to list the tray: {}", e),
      },
      Commands::Activate { address, menu } => {
        done(Activate::send((address, menu)), "activate the tray item")
      }
    }
  }
}

fn items() -> Vec<CompletionCandidate> {
  described(
    ListEntries::send(())
      .unwrap_or_default()
      .into_iter()
      .filter(|e| e.menu.is_none())
      .map(|e| (e.address, e.label)),
  )
}

fn menus() -> Vec<CompletionCandidate> {
  described(
    ListEntries::send(())
      .unwrap_or_default()
      .into_iter()
      .filter_map(|e| Some((e.menu?.to_string(), e.label))),
  )
}
