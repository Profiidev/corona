//! Trash, drives, processes and timers over IPC

use anyhow::{Context, Result, bail};
use corona_ipc::{IpcCommand, IpcServer, Reply};
use corona_launcher::commands::{
  processes::{by_port, kill, snapshot},
  storage::{empty_trash, trash_count},
  timers::{self, Kind},
};
use corona_udisks::{MountOptions, UdisksExt, UnmountOptions};
use gpui_kit::App;
use serde::{Deserialize, Serialize};

use crate::overlays::drive_password;

pub fn register_commands(server: &mut IpcServer) {
  server
    .register::<TrashCount>()
    .register::<EmptyTrash>()
    .register::<ListDrives>()
    .register::<DriveAction>()
    .register::<DrivePasswordPrompt>()
    .register::<KillProcess>()
    .register::<KillPort>()
    .register::<ListProcesses>()
    .register::<StartTimer>()
    .register::<ListTimers>()
    .register::<StopTimer>();
}

pub struct TrashCount;

impl IpcCommand for TrashCount {
  const COMMAND: &'static str = "trash:count";

  type Payload = ();
  type Response = usize;

  fn handle(_: Self::Payload, _: &mut App) -> Result<Self::Response> {
    trash_count()
  }
}

pub struct EmptyTrash;

impl IpcCommand for EmptyTrash {
  const COMMAND: &'static str = "trash:empty";

  type Payload = ();
  /// How many items went
  type Response = usize;

  fn handle(_: Self::Payload, _: &mut App) -> Result<Self::Response> {
    empty_trash()
  }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DriveInfo {
  pub device: String,
  pub label: String,
  pub mount_points: Vec<String>,
}

pub struct ListDrives;

impl IpcCommand for ListDrives {
  const COMMAND: &'static str = "drive:list";

  type Payload = ();
  type Response = Vec<DriveInfo>;

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    Ok(
      cx.udisks()
        .list_volumes(cx)
        .iter()
        .map(|v| DriveInfo {
          device: v.device.clone(),
          label: v.label.clone(),
          mount_points: v.mount_points.clone(),
        })
        .collect(),
    )
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
pub enum Drive {
  Mount,
  Unmount,
  Eject,
}

/// Mounts, unmounts or ejects a drive by device or label
pub struct DriveAction;

impl IpcCommand for DriveAction {
  const COMMAND: &'static str = "drive:action";

  type Payload = (Drive, String);
  type Response = ();

  fn handle(_: Self::Payload, _: &mut App) -> Result<Self::Response> {
    bail!("drive:action answers asynchronously")
  }

  fn reply((action, which): Self::Payload, cx: &mut App) -> Reply<Self::Response> {
    let udisks = cx.udisks();
    let volume = udisks
      .list_volumes(cx)
      .iter()
      .find(|v| v.device == which || v.label.eq_ignore_ascii_case(&which))
      .cloned()
      .with_context(|| format!("no drive {which}"));
    let volume = match volume {
      Ok(volume) => volume,
      Err(e) => return Box::pin(async move { Err(e) }),
    };
    match action {
      Drive::Mount if volume.is_locked() => {
        Box::pin(async move { bail!("{} is locked, unlock it first", volume.device) })
      }
      Drive::Mount => {
        let mount = udisks.mount(&volume.block, MountOptions::default());
        Box::pin(async move { mount.await.map(|_| ()) })
      }
      Drive::Unmount => Box::pin(udisks.unmount(&volume.block, UnmountOptions::default())),
      Drive::Eject => match &volume.drive {
        Some(drive) => Box::pin(udisks.eject_drive(drive, cx)),
        None => Box::pin(async move { bail!("{} is on no drive", volume.device) }),
      },
    }
  }
}

/// Asks for a drive's passphrase in a dialog, for another program unlocking it
pub struct DrivePasswordPrompt;

impl IpcCommand for DrivePasswordPrompt {
  const COMMAND: &'static str = "drive:password-prompt";

  /// The device, as shown in the dialog
  type Payload = String;
  /// None when cancelled
  type Response = Option<String>;

  fn handle(_: Self::Payload, _: &mut App) -> Result<Self::Response> {
    bail!("drive:password-prompt answers asynchronously")
  }

  fn reply(device: Self::Payload, cx: &mut App) -> Reply<Self::Response> {
    let asked = drive_password::ask(device, cx);
    Box::pin(async move { Ok(asked.await.ok()) })
  }
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
    let mut list: Vec<(u32, String)> = snapshot().into_iter().map(|p| (p.pid, p.name)).collect();
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
    let pids = by_port(port)?;
    for pid in &pids {
      kill(*pid, false)?;
    }
    Ok(pids)
  }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TimerRequest {
  /// None starts a stopwatch
  pub seconds: Option<u64>,
  pub label: Option<String>,
}

pub struct StartTimer;

impl IpcCommand for StartTimer {
  const COMMAND: &'static str = "timer:start";

  type Payload = TimerRequest;
  /// The new timer's id
  type Response = u64;

  fn handle(request: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let now = std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)?
      .as_millis() as u64;
    let kind = match request.seconds {
      Some(0) => bail!("expected at least a second"),
      Some(s) => Kind::Timer {
        end: now + s * 1000,
        total: s * 1000,
      },
      None => Kind::Stopwatch { start: now },
    };
    Ok(timers::start(kind, request.label, cx).id)
  }
}

pub struct ListTimers;

impl IpcCommand for ListTimers {
  const COMMAND: &'static str = "timer:list";

  type Payload = ();
  type Response = Vec<timers::Timer>;

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    Ok(timers::list(cx))
  }
}

pub struct StopTimer;

impl IpcCommand for StopTimer {
  const COMMAND: &'static str = "timer:stop";

  type Payload = u64;
  type Response = ();

  fn handle(id: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    if !timers::remove(id, cx) {
      bail!("no timer {id}");
    }
    Ok(())
  }
}
