//! Runs the lock and power actions of the idle behaviors

use std::rc::Rc;

use corona_config::IdleAction;
use corona_idle::SessionHook;
use gpui_kit::App;

use crate::ipc::auth::{Action, run};

pub(super) fn init(cx: &mut App) {
  cx.set_global(SessionHook(Rc::new(|action, cx| {
    let action = match action {
      IdleAction::Lock => Action::Lock,
      IdleAction::Suspend => Action::Suspend,
      IdleAction::LockAndSuspend => Action::LockAndSuspend,
      IdleAction::LockAndSuspendThenHibernate => Action::LockAndSuspendThenHibernate,
      IdleAction::ScreenOff | IdleAction::Command => return,
    };
    run(action, cx);
  })));
}
