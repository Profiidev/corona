//! Corona: a Hyprland desktop shell built on GPUI-CE layer-shell windows.
//!
//! [`Shell`] is the single piece of shared state. It is a GPUI entity, not an
//! `Rc<RefCell<_>>`: entity handles are `Send + Sync`, so they cross into
//! background tasks, and `update` hands out `&mut Self` without borrow panics.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gpui::{AppContext as _, Context, EventEmitter};

use crate::{dbus::Dbus, event::ShellEvent};

pub mod dbus;
pub mod event;
pub mod hypr;

pub use dbus::CloseReason;

pub struct Shell {
  /// Hyprland workspace names, in Hyprland's own order.
  pub workspaces: Vec<String>,
  /// `None` when no session bus was reachable at startup.
  dbus: Option<Dbus>,
}

/// Subscribe with `cx.subscribe(&shell, ..)` to observe the raw event stream.
impl EventEmitter<ShellEvent> for Shell {}

impl Shell {
  pub fn new(cx: &mut Context<Self>) -> Self {
    let (tx, rx) = smol::channel::unbounded::<ShellEvent>();

    hypr::spawn_listener(tx.clone());

    let dbus = match Dbus::init(tx) {
      Ok(dbus) => Some(dbus),
      Err(e) => {
        tracing::warn!("notification server unavailable: {e:#}");
        None
      }
    };

    cx.spawn(async move |shell, cx| {
      while let Ok(event) = rx.recv().await {
        if shell
          .update(cx, |shell, cx| shell.handle(event, cx))
          .is_err()
        {
          break;
        }
      }
    })
    .detach();

    cx.spawn(async move |shell, cx| {
      loop {
        cx.background_executor().timer(until_next_second()).await;
        if shell
          .update(cx, |shell, cx| shell.handle(ShellEvent::Tick, cx))
          .is_err()
        {
          break;
        }
      }
    })
    .detach();

    Self::refresh_workspaces(cx);

    Self {
      workspaces: Vec::new(),
      dbus,
    }
  }

  pub fn signal_notification_closed(&self, id: u32, reason: CloseReason) {
    if let Some(dbus) = &self.dbus {
      dbus.notification_closed(id, reason);
    }
  }

  pub fn signal_notification_action(&self, id: u32, action: &str) {
    if let Some(dbus) = &self.dbus {
      dbus.notification_action(id, action);
    }
  }

  fn handle(&mut self, event: ShellEvent, cx: &mut Context<Self>) {
    tracing::debug!("shell event: {event:?}");
    // ponytail: any workspace change refetches the whole list. It is one cheap
    // socket round trip; track incrementally only if it ever shows up in a profile.
    if matches!(event, ShellEvent::Workspace(_)) {
      Self::refresh_workspaces(cx);
    }

    cx.emit(event);
    cx.notify();
  }

  fn refresh_workspaces(cx: &mut Context<Self>) {
    cx.spawn(async move |shell, cx| {
      let names = cx.background_spawn(async { hypr::workspace_names() }).await;

      match names {
        Ok(names) => {
          let _ = shell.update(cx, |shell, cx| {
            if shell.workspaces != names {
              shell.workspaces = names;
              cx.notify();
            }
          });
        }
        Err(e) => tracing::warn!("failed to list workspaces: {e:#}"),
      }
    })
    .detach();
  }
}

/// Time until the next wall-clock second boundary, so the clock ticks in step
/// with the displayed time rather than drifting from process start.
fn until_next_second() -> Duration {
  let subsec = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap_or_default()
    .subsec_nanos() as u64;
  // 1ms skew to prevent a double trigger
  Duration::from_nanos(1_000_000_000 - subsec) + Duration::from_millis(1)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn lands_after_a_second_boundary() {
    for _ in 0..100 {
      let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
      let target = now + until_next_second();
      assert_eq!(target.as_secs(), now.as_secs() + 1);
      assert!(target.subsec_nanos() < 2_000_000, "{target:?}");
      std::thread::sleep(Duration::from_millis(7));
    }
  }
}
