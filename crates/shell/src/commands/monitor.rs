use anyhow::Result;
use clap::ValueEnum;
use corona_compositor::{Compositor, types::MonitorChange};
use corona_ipc::{IpcCommand, IpcServer};
use gpui_kit::App;
use serde::{Deserialize, Serialize};

pub fn register_commands(server: &mut IpcServer) {
  server.register::<Configure>();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
pub enum Change {
  Enable,
  Disable,
  Mirror,
  StopMirroring,
}

/// Enables, disables or mirrors a monitor until the compositor reloads its config
pub struct Configure;

impl IpcCommand for Configure {
  const COMMAND: &'static str = "monitor:configure";

  /// The monitor, the change and for mirroring the monitor to show
  type Payload = (String, Change, Option<String>);
  type Response = ();

  fn handle((name, change, source): Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let change = match change {
      Change::Enable => MonitorChange::Enable,
      Change::Disable => MonitorChange::Disable,
      Change::Mirror => MonitorChange::Mirror(
        source.ok_or_else(|| anyhow::anyhow!("mirroring needs the monitor to show"))?,
      ),
      Change::StopMirroring => MonitorChange::StopMirroring,
    };
    Compositor::configure_monitor(&name, change, cx)
  }
}
