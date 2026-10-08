use anyhow::Result;
use clap::ValueEnum;
use corona_ipc::{IpcCommand, IpcServer};
use corona_power::{PowerExt, SessionAction};
use corona_utils::error::ErrorLogExt;
use gpui_kit::App;
use serde::{Deserialize, Serialize};

use crate::lock::LockState;

pub fn register_commands(server: &mut IpcServer) {
  server.register::<Session>();
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, ValueEnum)]
pub enum Action {
  /// Lock the current session
  Lock,
  /// Suspend without locking first
  Suspend,
  /// Lock the session, then suspend once the lock is active
  LockAndSuspend,
  /// Suspend, then hibernate after the time logind sets (`HibernateDelaySec`)
  SuspendThenHibernate,
  /// Lock the session, then suspend and later hibernate
  LockAndSuspendThenHibernate,
  /// End the graphical session
  Logout,
  /// Reboot the system
  Reboot,
  /// Shut down the system
  Shutdown,
}

pub struct Session;

impl IpcCommand for Session {
  const COMMAND: &'static str = "session:action";

  type Payload = Action;
  type Response = ();

  fn handle(action: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let power = match action {
      Action::Lock => None,
      Action::Suspend | Action::LockAndSuspend => Some(SessionAction::Suspend),
      Action::SuspendThenHibernate | Action::LockAndSuspendThenHibernate => {
        Some(SessionAction::SuspendThenHibernate)
      }
      Action::Logout => Some(SessionAction::Logout),
      Action::Reboot => Some(SessionAction::Reboot),
      Action::Shutdown => Some(SessionAction::PowerOff),
    };
    let lock = matches!(
      action,
      Action::Lock | Action::LockAndSuspend | Action::LockAndSuspendThenHibernate
    )
    .then(|| LockState::lock(cx));
    let power = power.map(|a| cx.power().session_action(a));

    cx.spawn(async move |_| {
      if let Some(lock) = lock {
        lock.await;
      }
      if let Some(power) = power {
        let _ = power.await.log_err();
      }
    })
    .detach();
    Ok(())
  }
}
