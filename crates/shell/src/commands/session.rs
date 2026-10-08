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

/// Whether to lock first and which power action follows
fn plan(action: Action) -> (bool, Option<SessionAction>) {
  let lock = matches!(
    action,
    Action::Lock | Action::LockAndSuspend | Action::LockAndSuspendThenHibernate
  );
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
  (lock, power)
}

pub struct Session;

impl IpcCommand for Session {
  const COMMAND: &'static str = "session:action";

  type Payload = Action;
  type Response = ();

  fn handle(action: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let (lock, power) = plan(action);
    let lock = lock.then(|| LockState::lock(cx));
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn plans() {
    use SessionAction as S;
    let cases = [
      (Action::Lock, true, None),
      (Action::Suspend, false, Some(S::Suspend)),
      (Action::LockAndSuspend, true, Some(S::Suspend)),
      (
        Action::SuspendThenHibernate,
        false,
        Some(S::SuspendThenHibernate),
      ),
      (
        Action::LockAndSuspendThenHibernate,
        true,
        Some(S::SuspendThenHibernate),
      ),
      (Action::Logout, false, Some(S::Logout)),
      (Action::Reboot, false, Some(S::Reboot)),
      (Action::Shutdown, false, Some(S::PowerOff)),
    ];
    assert_eq!(cases.len(), Action::value_variants().len());
    for (action, lock, power) in cases {
      assert_eq!(plan(action), (lock, power), "{action:?}");
    }
  }
}
