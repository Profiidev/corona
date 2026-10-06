use clap::Subcommand;
use clap_complete::{ArgValueCandidates, CompletionCandidate};
use corona_ipc::IpcCommandSend;
use corona_shell::commands::brightness::{Action as BrightnessAction, ListMonitors, SetBrightness};

use crate::ipc::{STEP, candidates, level};

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
pub enum BrightnessCommands {
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
}

impl BrightnessCommands {
  pub fn execute(self) {
    let (monitor, action) = match self {
      BrightnessCommands::Set { level, monitor } => (monitor, BrightnessAction::Set(level)),
      BrightnessCommands::Up { step, monitor } => (monitor, BrightnessAction::Change(step)),
      BrightnessCommands::Down { step, monitor } => (monitor, BrightnessAction::Change(-step)),
    };
    if let Err(e) = SetBrightness::send((monitor.monitor, action)) {
      tracing::error!("Failed to change brightness: {}", e);
    }
  }
}
