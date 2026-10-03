use std::{
  collections::HashMap,
  sync::atomic::{AtomicU32, Ordering},
  time::SystemTime,
};

use zbus::{interface, object_server::SignalEmitter, zvariant::OwnedValue};

use crate::state::{self, Notification, Urgency};

pub(crate) const PATH: &str = "/org/freedesktop/Notifications";
pub(crate) const NAME: &str = "org.freedesktop.Notifications";

#[derive(Clone, Copy)]
#[repr(u32)]
pub(crate) enum CloseReason {
  Dismissed = 2,
  Closed = 3,
}

pub(crate) enum Event {
  Notify(Notification),
  Close(u32),
}

pub(crate) struct Server {
  pub events: flume::Sender<Event>,
  pub next_id: AtomicU32,
}

fn hint<T: TryFrom<OwnedValue>>(hints: &HashMap<String, OwnedValue>, key: &str) -> Option<T> {
  T::try_from(hints.get(key)?.try_clone().ok()?).ok()
}

#[interface(name = "org.freedesktop.Notifications")]
impl Server {
  fn get_capabilities(&self) -> Vec<&'static str> {
    vec!["body", "actions", "persistence"]
  }

  #[allow(clippy::too_many_arguments)]
  fn notify(
    &self,
    app_name: String,
    replaces_id: u32,
    app_icon: String,
    summary: String,
    body: String,
    actions: Vec<String>,
    hints: HashMap<String, OwnedValue>,
    _expire_timeout: i32,
  ) -> u32 {
    let id = match replaces_id {
      0 => self.next_id.fetch_add(1, Ordering::Relaxed),
      id => id,
    };
    let urgency = match hint::<u8>(&hints, "urgency") {
      Some(0) => Urgency::Low,
      Some(2) => Urgency::Critical,
      _ => Urgency::Normal,
    };
    let notification = Notification {
      id,
      app_name,
      app_icon,
      summary,
      body,
      actions: state::actions(actions),
      urgency,
      desktop_entry: hint(&hints, "desktop-entry"),
      resident: hint(&hints, "resident").unwrap_or(false),
      time: SystemTime::now(),
      read: false,
    };
    let _ = self.events.send(Event::Notify(notification));
    id
  }

  async fn close_notification(
    &self,
    id: u32,
    #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
  ) -> zbus::fdo::Result<()> {
    let _ = self.events.send(Event::Close(id));
    Self::notification_closed(&emitter, id, CloseReason::Closed as u32).await?;
    Ok(())
  }

  fn get_server_information(&self) -> (&'static str, &'static str, &'static str, &'static str) {
    ("corona", "corona", env!("CARGO_PKG_VERSION"), "1.2")
  }

  #[zbus(signal)]
  pub(crate) async fn notification_closed(
    emitter: &SignalEmitter<'_>,
    id: u32,
    reason: u32,
  ) -> zbus::Result<()>;

  #[zbus(signal)]
  pub(crate) async fn action_invoked(
    emitter: &SignalEmitter<'_>,
    id: u32,
    action_key: &str,
  ) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
  use super::*;

  /// talks to the server over this session bus by its unique name, so the real
  /// `org.freedesktop.Notifications` owner is left alone:
  /// `cargo test -p corona_notifications -- --ignored`
  #[test]
  #[ignore]
  fn notify_over_dbus() {
    zbus::block_on(async {
      let (events, received) = flume::unbounded();
      let server = zbus::Connection::session().await.unwrap();
      server
        .object_server()
        .at(
          PATH,
          Server {
            events,
            next_id: AtomicU32::new(7),
          },
        )
        .await
        .unwrap();

      let client = zbus::Connection::session().await.unwrap();
      let hints: HashMap<&str, zbus::zvariant::Value> =
        HashMap::from([("urgency", 2u8.into()), ("desktop-entry", "nixos".into())]);
      let reply = client
        .call_method(
          server.unique_name().map(|n| n.to_string()).as_deref(),
          PATH,
          Some(NAME),
          "Notify",
          &(
            "Nix",
            0u32,
            "",
            "Rebuild finished",
            "12 derivations built",
            vec!["default", "Open"],
            hints,
            -1i32,
          ),
        )
        .await
        .unwrap();
      let id: u32 = reply.body().deserialize().unwrap();
      assert_eq!(id, 7);

      let Event::Notify(notification) = received.recv_async().await.unwrap() else {
        panic!("expected a notification");
      };
      assert_eq!(notification.summary, "Rebuild finished");
      assert_eq!(notification.urgency, Urgency::Critical);
      assert_eq!(notification.desktop_entry.as_deref(), Some("nixos"));
      assert_eq!(notification.actions[0].key, "default");
    });
  }
}
