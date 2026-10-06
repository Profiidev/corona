use anyhow::Result;
use clap::ValueEnum;
use corona_ipc::{IpcCommand, IpcServer};
use gpui_kit::{App, Modifiers};
use serde::{Deserialize, Serialize};

use crate::overlays::switcher::SwitcherState;

pub fn register_commands(server: &mut IpcServer) {
  server.register::<Cycle>();
}

pub struct Cycle;

impl IpcCommand for Cycle {
  const COMMAND: &'static str = "switcher:cycle";

  type Payload = Options;
  type Response = ();

  fn handle(options: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    SwitcherState::cycle(options, cx)
  }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Options {
  pub mode: Mode,
  pub modifier: Modifier,
  pub current_monitor: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
pub enum Mode {
  #[default]
  Window,
  Workspace,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
pub enum Modifier {
  #[default]
  Super,
  Alt,
  Ctrl,
}

impl Modifier {
  pub(super) fn held(self, modifiers: &Modifiers) -> bool {
    match self {
      Modifier::Super => modifiers.platform,
      Modifier::Alt => modifiers.alt,
      Modifier::Ctrl => modifiers.control,
    }
  }
}
