use clap::Subcommand;

use corona_ipc::IpcCommandSend;
use corona_shell::{
  commands::radio::Switch,
  overlays::osk::{Hide, Show, Tablet, Toggle},
};

use super::print_switch;

#[derive(Subcommand)]
pub enum OskCommands {
  Show,
  Hide,
  Toggle,
  /// Tablet mode, for a Hyprland `switch:` bind: on shows the keyboard when a
  /// text field has focus, off hides it
  Tablet {
    switch: Switch,
  },
}

impl OskCommands {
  pub fn execute(self) {
    let result = match self {
      OskCommands::Show => Show::send(()),
      OskCommands::Hide => Hide::send(()),
      OskCommands::Toggle => Toggle::send(()),
      OskCommands::Tablet { switch } => {
        return print_switch(Tablet::send(switch), switch, "tablet mode");
      }
    };
    if let Err(e) = result {
      tracing::error!("Failed to control the on-screen keyboard: {}", e);
    }
  }
}
