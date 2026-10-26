use clap::Subcommand;
use corona_ipc::IpcCommandSend;
use corona_shell::commands::{
  nightlight::{Nightlight, Request},
  radio::Switch,
};

#[derive(Subcommand)]
pub enum NightlightCommands {
  /// Turn it on
  On,
  /// Turn it off
  Off,
  /// Toggle it
  Toggle,
  /// Print on or off and the temperature
  Status,
  /// Set the night temperature in kelvin, 1000 to 6500
  Temp { kelvin: u32 },
}

impl NightlightCommands {
  pub fn execute(self) {
    let request = match self {
      NightlightCommands::On => switch(Switch::On),
      NightlightCommands::Off => switch(Switch::Off),
      NightlightCommands::Toggle => switch(Switch::Toggle),
      NightlightCommands::Status => switch(Switch::Status),
      NightlightCommands::Temp { kelvin } => Request {
        switch: None,
        temperature: Some(kelvin),
      },
    };
    match Nightlight::send(request) {
      Ok(status) if matches!(request.switch, Some(Switch::Status)) => {
        let on = if status.on { "on" } else { "off" };
        println!("{on} {}K", status.temperature);
      }
      Ok(_) => {}
      Err(e) => tracing::error!("Failed to change the nightlight: {}", e),
    }
  }
}

fn switch(switch: Switch) -> Request {
  Request {
    switch: Some(switch),
    temperature: None,
  }
}
