//! Killing processes, by pid or by port, and listing them

use anyhow::Result;
use corona_ipc::{IpcCommand, IpcExt};
use gpui_kit::App;

use corona_sysinfo::processes::{kill, kill_port, snapshot};

pub(crate) fn register(cx: &mut App) {
  cx.ipc_mut()
    .register::<KillProcess>()
    .register::<KillPort>()
    .register::<ListProcesses>();
}

pub struct KillProcess;

impl IpcCommand for KillProcess {
  const COMMAND: &'static str = "process:kill";

  /// The pid and whether to force
  type Payload = (u32, bool);
  type Response = ();

  fn handle((pid, force): Self::Payload, _: &mut App) -> Result<Self::Response> {
    kill(pid, force)
  }
}

/// Process ids with their names
pub struct ListProcesses;

impl IpcCommand for ListProcesses {
  const COMMAND: &'static str = "process:list";

  type Payload = ();
  type Response = Vec<(u32, String)>;

  fn handle(_: Self::Payload, _: &mut App) -> Result<Self::Response> {
    let mut list: Vec<(u32, String)> = snapshot()?.into_iter().map(|p| (p.pid, p.name)).collect();
    list.sort();
    Ok(list)
  }
}

pub struct KillPort;

impl IpcCommand for KillPort {
  const COMMAND: &'static str = "process:kill_port";

  type Payload = u16;
  /// The pids that got the signal
  type Response = Vec<u32>;

  fn handle(port: Self::Payload, _: &mut App) -> Result<Self::Response> {
    kill_port(port)
  }
}
