//! `corona ipc idle`

use clap::Subcommand;
use corona_ipc::{IpcCommandSend, Switch, cli::print_switch};

use crate::commands::{Request, StayAwake};

#[derive(Subcommand)]
pub enum Commands {
  /// Keep the session awake, no idle behaviors and no sleep: on, off, toggle
  /// or status
  StayAwake {
    /// Toggle by default, on with `--for`
    switch: Option<Switch>,
    /// Turn it off again after this many minutes
    #[arg(long = "for", value_name = "MINUTES")]
    minutes: Option<u32>,
  },
}

impl Commands {
  pub fn run(self) {
    match self {
      Commands::StayAwake { switch, minutes } => {
        let switch = switch.unwrap_or(match minutes {
          Some(_) => Switch::On,
          None => Switch::Toggle,
        });
        let request = Request { switch, minutes };
        print_switch(StayAwake::send(request), switch, "stay awake")
      }
    }
  }
}
