use anyhow::{Context, Result};
use corona_ipc::{IpcCommand, IpcServer};
use corona_launcher::commands::calculator::calculate;
use gpui_kit::App;

pub fn register_commands(server: &mut IpcServer) {
  server.register::<Evaluate>();
}

/// What the launcher's calculator makes of an expression
pub struct Evaluate;

impl IpcCommand for Evaluate {
  const COMMAND: &'static str = "calc:eval";

  type Payload = String;
  type Response = String;

  fn handle(input: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    calculate(&input, cx).context("not a calculation")
  }
}
