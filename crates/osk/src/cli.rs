//! `corona ipc osk`

use clap::Subcommand;
use corona_ipc::{IpcCommandSend, Switch, cli::print_switch};

use crate::commands::{Hide, Show, Tablet, Toggle};

#[derive(Subcommand)]
pub enum Commands {
  Show,
  Hide,
  Toggle,
  /// Tablet mode, for a Hyprland `switch:` bind: on shows the keyboard when a
  /// text field has focus, off hides it
  Tablet {
    switch: Switch,
  },
}

impl Commands {
  pub fn run(self) {
    let result = match self {
      Commands::Show => Show::send(()),
      Commands::Hide => Hide::send(()),
      Commands::Toggle => Toggle::send(()),
      Commands::Tablet { switch } => {
        return print_switch(Tablet::send(switch), switch, "tablet mode");
      }
    };
    if let Err(e) = result {
      tracing::error!("Failed to control the on-screen keyboard: {}", e);
    }
  }
}
