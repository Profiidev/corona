use std::collections::HashMap;

use zbus::interface;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::OwnedValue;

use crate::event::event::ShellEvent;
use crate::event::event_loop::ShellSender;

const PATH: &str = "/org/freedesktop/Notifications";
const NAME: &str = "org.freedesktop.Notifications";

#[derive(Clone, Copy)]
pub enum CloseReason {
  Expired = 1,
  Dismissed = 2,
  Closed = 3,
  Undefined = 4,
}

pub struct Dbus(zbus::blocking::Connection);

impl Dbus {
  pub fn init(tx: ShellSender) -> zbus::Result<Self> {
    let notifications = Notifications { tx, next_id: 0 };

    let conn = zbus::blocking::connection::Builder::session()?
      .serve_at(PATH, notifications)?
      .build()?;

    if let Err(e) = conn.request_name(NAME) {
      tracing::warn!("Failed to request name {}: {}", NAME, e);
    }

    Ok(Self(conn))
  }

  pub fn destroy(self) {
    self.0.graceful_shutdown();
  }

  pub fn notification_closed(&self, id: u32, reason: CloseReason) {
    self.emit(|e| zbus::block_on(Notifications::notification_closed(e, id, reason as u32)));
  }

  pub fn notification_action(&self, id: u32, action_key: &str) {
    self.emit(|e| zbus::block_on(Notifications::action_invoked(e, id, action_key)));
  }

  fn emit<F: FnOnce(&SignalEmitter<'_>) -> zbus::Result<()>>(&self, f: F) {
    let result = SignalEmitter::new(self.0.inner(), PATH).and_then(|e| f(&e));

    if let Err(e) = result {
      tracing::warn!("Failed to emit signal: {e:#}");
    }
  }
}

struct Notifications {
  tx: ShellSender,
  next_id: u32,
}

#[interface(name = "org.freedesktop.Notifications")]
impl Notifications {
  #[allow(clippy::too_many_arguments)]
  fn notify(
    &mut self,
    app_name: String,
    replaces_id: u32,
    app_icon: String,
    summary: String,
    body: String,
    actions: Vec<String>,
    _hints: HashMap<String, OwnedValue>,
    expire_timeout: i32,
  ) -> u32 {
    let id = if replaces_id != 0 {
      replaces_id
    } else {
      self.next_id += 1;
      self.next_id
    };

    let timeout_ms = if expire_timeout > 0 {
      expire_timeout
    } else {
      5000
    };

    let _ = self.tx.send(ShellEvent::NewNotification {
      id,
      app_name,
      summary,
      body,
      timeout_ms,
      app_icon,
      actions,
    });

    id
  }

  fn close_notification(&mut self, id: u32) {
    let _ = self.tx.send(ShellEvent::CloseNotification(id));
  }

  #[zbus(signal)]
  async fn notification_closed(
    emitter: &SignalEmitter<'_>,
    id: u32,
    reason: u32,
  ) -> zbus::Result<()>;

  #[zbus(signal)]
  async fn action_invoked(
    emitter: &SignalEmitter<'_>,
    id: u32,
    action_key: &str,
  ) -> zbus::Result<()>;

  fn get_capabilities(&self) -> Vec<String> {
    vec!["body".into(), "actions".into()]
  }

  fn get_server_information(&self) -> (String, String, String, String) {
    (
      env!("CARGO_PKG_NAME").into(),
      env!("CARGO_PKG_AUTHORS").into(),
      env!("CARGO_PKG_VERSION").into(),
      "1.3".into(),
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Emitting runs on the event loop thread, so it must not be a round trip.
  #[test]
  fn emitting_a_signal_does_not_block() {
    let (tx, _rx) = calloop::channel::channel();
    let Ok(dbus) = Dbus::init(tx) else {
      return; // no session bus (CI)
    };
    dbus.notification_closed(1, CloseReason::Dismissed);

    let start = std::time::Instant::now();
    for id in 0..1000 {
      dbus.notification_closed(id, CloseReason::Dismissed);
    }

    let per = start.elapsed() / 1000;
    assert!(
      per < std::time::Duration::from_millis(1),
      "{per:?} per emit"
    );
  }
}
