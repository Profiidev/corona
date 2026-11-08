//! Runs the lock and power actions of the idle behaviors and keeps logind awake with them

use corona_config::IdleAction;
use corona_idle::{IdleEvent, IdleExt};
use corona_power::PowerExt;
use gpui_kit::App;

use crate::ipc::auth::{Action, run};

pub(super) fn init(cx: &mut App) {
  // logind's own idle action and sleep wait too
  let stay_awake = cx.idle().stay_awake.clone();
  cx.power().clone().stay_awake_while(&stay_awake, cx);
  let events = cx.idle().events.clone();
  cx.subscribe(&events, |_, IdleEvent::Fired { action, .. }, cx| {
    if let Some(action) = session_action(*action) {
      run(action, cx);
    }
  })
  .detach();
}

fn session_action(action: IdleAction) -> Option<Action> {
  match action {
    IdleAction::Lock => Some(Action::Lock),
    IdleAction::Suspend => Some(Action::Suspend),
    IdleAction::LockAndSuspend => Some(Action::LockAndSuspend),
    IdleAction::LockAndSuspendThenHibernate => Some(Action::LockAndSuspendThenHibernate),
    IdleAction::ScreenOff | IdleAction::Command => None,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn only_session_actions_run() {
    assert!(matches!(
      session_action(IdleAction::Lock),
      Some(Action::Lock)
    ));
    assert!(session_action(IdleAction::ScreenOff).is_none());
    assert!(session_action(IdleAction::Command).is_none());
  }
}
