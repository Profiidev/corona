//! Lock, suspend, hibernate, log out, reboot and shut down

use corona_ipc::IpcCommand;
use corona_launcher::{Action, Confirm, Icon, Meta, Outcome, kit::Group, simple};
use gpui_kit::{App, assets::IconName};
use rust_i18n::t;

use crate::commands::session::{Action as SessionAction, Session};

pub fn init(cx: &mut App) {
  use SessionAction::*;
  let commands = [
    ("session:lock", Lock, IconName::Lock, false),
    ("session:suspend", Suspend, IconName::Moon, false),
    (
      "session:lock_suspend",
      LockAndSuspend,
      IconName::Moon,
      false,
    ),
    (
      "session:hibernate",
      SuspendThenHibernate,
      IconName::Snowflake,
      false,
    ),
    (
      "session:lock_hibernate",
      LockAndSuspendThenHibernate,
      IconName::Snowflake,
      false,
    ),
    ("session:logout", Logout, IconName::LogOut, true),
    ("session:reboot", Reboot, IconName::RotateCw, true),
    ("session:shutdown", Shutdown, IconName::Power, true),
  ]
  .into_iter()
  .map(|(id, action, icon, confirm)| {
    simple(
      id,
      move |_| Meta {
        title: title(action).into(),
        icon: Icon::Tile(icon, 0.),
        keywords: vec!["session".into(), "power".into()],
        ..Default::default()
      },
      move |_| {
        let run = Action::sync(id, title(action), move |_, _, cx| {
          Session::handle(action, cx)?;
          Ok(Outcome::close())
        });
        let run = match confirm {
          true => run.destructive().confirm(Confirm {
            title: t!("app.launcher.session.confirm_title", action = title(action)).into(),
            message: t!("app.launcher.session.confirm_message").into(),
            button: title(action).into(),
            skippable: true,
          }),
          false => run,
        };
        vec![run]
      },
    )
  })
  .collect();
  Group::new("session", commands).register(cx);
}

fn title(action: SessionAction) -> String {
  use SessionAction::*;
  match action {
    Lock => t!("app.launcher.session.lock"),
    Suspend => t!("app.launcher.session.suspend"),
    LockAndSuspend => t!("app.launcher.session.lock_suspend"),
    SuspendThenHibernate => t!("app.launcher.session.hibernate"),
    LockAndSuspendThenHibernate => t!("app.launcher.session.lock_hibernate"),
    Logout => t!("app.launcher.session.logout"),
    Reboot => t!("app.launcher.session.reboot"),
    Shutdown => t!("app.launcher.session.shutdown"),
  }
  .to_string()
}
