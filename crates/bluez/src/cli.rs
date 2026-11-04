//! `corona ipc bluetooth`

use clap::{Subcommand, ValueEnum};
use clap_complete::{ArgValueCandidates, CompletionCandidate};
use corona_ipc::{
  IpcCommandSend, Switch,
  cli::{described, done, print_switch},
};

use crate::commands::{Action, Bluetooth, DeviceAction, Discover, ListDevices};

#[derive(Subcommand)]
pub enum Commands {
  /// Bluetooth adapter: on, off, toggle or status
  Power { switch: Switch },
  /// List Bluetooth devices
  Devices,
  /// Connect, disconnect, pair or forget a Bluetooth device by address or name
  Device {
    #[arg(ignore_case = true)]
    action: Action,
    #[arg(add = ArgValueCandidates::new(devices))]
    device: String,
  },
  /// Look for Bluetooth devices to pair: on or off
  Discover { state: Discovery },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Discovery {
  On,
  Off,
}

impl Commands {
  pub fn run(self) {
    match self {
      Commands::Power { switch } => print_switch(Bluetooth::send(switch), switch, "Bluetooth"),
      Commands::Devices => match ListDevices::send(()) {
        Ok(devices) => {
          for d in devices {
            let state = if d.connected {
              "connected"
            } else if d.paired {
              "paired"
            } else {
              ""
            };
            let battery = d.battery.map(|b| format!(" {b}%")).unwrap_or_default();
            println!("{} {state:<9} {}{battery}", d.address, clean(&d.name));
          }
        }
        Err(e) => tracing::error!("Failed to list Bluetooth devices: {}", e),
      },
      Commands::Device { action, device } => done(
        DeviceAction::send((action, device)),
        "run the Bluetooth action",
      ),
      Commands::Discover { state } => done(
        Discover::send(matches!(state, Discovery::On)),
        "switch Bluetooth discovery",
      ),
    }
  }
}

/// Remote names are attacker-controlled, keep escape sequences out of the terminal
fn clean(name: &str) -> String {
  name
    .chars()
    .map(|c| if c.is_control() { '?' } else { c })
    .collect()
}

fn devices() -> Vec<CompletionCandidate> {
  described(
    ListDevices::send(())
      .unwrap_or_default()
      .into_iter()
      .map(|d| (d.address, clean(&d.name))),
  )
}
