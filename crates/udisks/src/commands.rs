//! Drives over IPC

use crate::{MountOptions, UdisksExt, UnmountOptions};
use anyhow::{Context, Result, bail};
use corona_ipc::{IpcCommand, IpcExt, Reply};
use gpui_kit::App;
use serde::{Deserialize, Serialize};

use crate::ui::drive_password;

pub(crate) fn register(cx: &mut App) {
  cx.ipc_mut()
    .register::<ListDrives>()
    .register::<DriveAction>()
    .register::<DrivePasswordPrompt>();
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
