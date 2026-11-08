use anyhow::Result;
use clap::ValueEnum;
use corona_ipc::{IpcCommand, IpcExt};
use corona_notifications::NotificationsExt;
use corona_utils::error::ErrorLogExt;
use gpui_kit::App;
use serde::{Deserialize, Serialize};

pub(crate) fn register(cx: &mut App) {
  cx.ipc_mut()
    .register::<DoNotDisturb>()
    .register::<Show>()
    .register::<ClearHistory>()
    .register::<DismissToasts>();
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, ValueEnum)]
pub enum Dnd {
  /// Hide current toasts and suppress new ones, history still fills
  On,
  /// Allow new toasts, suppressed ones are not replayed
  Off,
  /// Toggle toast suppression
  Toggle,
  /// Print the current state
  Status,
}

pub struct DoNotDisturb;

impl IpcCommand for DoNotDisturb {
  const COMMAND: &'static str = "notification:dnd";

  type Payload = Dnd;
  /// the state afterwards
  type Response = bool;

  fn handle(dnd: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let notifications = cx.notifications().clone();
    let enabled = match dnd {
      Dnd::On => true,
      Dnd::Off => false,
      Dnd::Toggle => !notifications.do_not_disturb(cx),
      Dnd::Status => return Ok(notifications.do_not_disturb(cx)),
    };
    notifications.set_do_not_disturb(enabled, cx);
    Ok(enabled)
  }
}

pub struct Show;

impl IpcCommand for Show {
  const COMMAND: &'static str = "notification:show";

  type Payload = String;
  type Response = ();

  fn handle(summary: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let send = cx.notifications().send(summary, String::new());
    cx.spawn(async move |_| {
      let _ = send.await.log_err();
    })
    .detach();
    Ok(())
  }
}

pub struct ClearHistory;

impl IpcCommand for ClearHistory {
  const COMMAND: &'static str = "notification:clear_history";

  type Payload = ();
  type Response = ();

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    cx.notifications().clone().clear_all(cx);
    Ok(())
  }
}

/// Hides the toasts on screen, the history keeps them
pub struct DismissToasts;

impl IpcCommand for DismissToasts {
  const COMMAND: &'static str = "notification:dismiss";

  type Payload = ();
  type Response = ();

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    crate::overlay::notifications::NotificationPopups::dismiss_all(cx);
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[gpui_kit::test]
  fn dnd_show_and_clear(cx: &mut gpui_kit::TestAppContext) {
    cx.executor().allow_parking();
    let (_bus, conn) = corona_utils::test_bus::TestBus::with_conn();
    cx.update(|cx| {
      let ex = cx.foreground_executor().clone();
      ex.block_on(corona_notifications::connect(cx, &conn, true))
        .unwrap();
      assert!(!DoNotDisturb::handle(Dnd::Status, cx).unwrap());
      assert!(DoNotDisturb::handle(Dnd::On, cx).unwrap());
      assert!(DoNotDisturb::handle(Dnd::Status, cx).unwrap());
      assert!(!DoNotDisturb::handle(Dnd::Toggle, cx).unwrap());
      assert!(!DoNotDisturb::handle(Dnd::Off, cx).unwrap());
      assert!(Show::handle("hi".into(), cx).is_ok());
      assert!(ClearHistory::handle((), cx).is_ok());
    });
  }
}
