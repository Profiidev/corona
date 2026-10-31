//! `corona ipc brightness`

use clap::Subcommand;
use clap_complete::{ArgValueCandidates, CompletionCandidate};
use corona_ipc::{
  IpcCommandSend, Switch,
  cli::{STEP, candidates, level},
};

use crate::commands::{
  brightness::{Action, ListMonitors, SetBrightness},
  nightlight::{Nightlight, Request},
};

#[derive(clap::Args)]
pub struct MonitorArg {
  /// The monitor's output name, like DP-1, or * for all; the focused one when left out
  #[arg(short, long, add = ArgValueCandidates::new(monitor_names))]
  monitor: Option<String>,
}

/// Asks the running shell for the displays it can dim.
fn monitor_names() -> Vec<CompletionCandidate> {
  let mut names = candidates(ListMonitors::send(()));
  names.push(CompletionCandidate::new("*"));
  names
}

#[derive(Subcommand)]
pub enum Commands {
  /// Set the brightness: 65, 65% or 0.65
  Set {
    #[arg(value_parser = level)]
    level: f32,
    #[command(flatten)]
    monitor: MonitorArg,
  },
  /// Raise the brightness
  Up {
    /// The step: 10, 10% or 0.1
    #[arg(value_parser = level, default_value = STEP)]
    step: f32,
    #[command(flatten)]
    monitor: MonitorArg,
  },
  /// Lower the brightness
  Down {
    /// The step: 10, 10% or 0.1
    #[arg(value_parser = level, default_value = STEP)]
    step: f32,
    #[command(flatten)]
    monitor: MonitorArg,
  },
  /// Nightlight: on, off, toggle, status or temp <KELVIN>
  Nightlight {
    #[command(subcommand)]
    command: NightlightCommands,
  },
}

impl Commands {
  pub fn run(self) {
    let (monitor, action) = match self {
      Commands::Set { level, monitor } => (monitor, Action::Set(level)),
      Commands::Up { step, monitor } => (monitor, Action::Change(step)),
      Commands::Down { step, monitor } => (monitor, Action::Change(-step)),
      Commands::Nightlight { command } => return command.run(),
    };
    if let Err(e) = SetBrightness::send((monitor.monitor, action)) {
      tracing::error!("Failed to change brightness: {}", e);
    }
  }
}

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
  pub fn run(self) {
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

#[cfg(test)]
mod tests {
  use corona_ipc::{IpcCommand, cli::testing::request};
  use serde_json::json;

  use super::*;

  #[test]
  fn monitor_completion_adds_all() {
    let mut names = vec![];
    let got = request(json!(["DP-1"]), || names = monitor_names());
    assert_eq!(got["command"], ListMonitors::COMMAND);
    let names: Vec<_> = names.iter().map(|c| c.get_value().to_owned()).collect();
    assert_eq!(names, ["DP-1", "*"]);
  }
}
