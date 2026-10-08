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

impl Server {
  /// counts up from 1, skipping 0 which the spec reserves for "no notification"
  fn fresh_id(&self) -> u32 {
    let next = |id: u32| Some(id.checked_add(1).unwrap_or(1));
    self
      .next_id
      .try_update(Ordering::Relaxed, Ordering::Relaxed, next)
      .unwrap_or(1)
  }
}

/// The spec says byte, plenty of clients send another integer type
fn urgency(hints: &HashMap<String, OwnedValue>) -> Option<i64> {
  hint::<u8>(hints, "urgency")
    .map(i64::from)
    .or_else(|| hint::<i32>(hints, "urgency").map(i64::from))
    .or_else(|| hint::<u32>(hints, "urgency").map(i64::from))
    .or_else(|| hint::<i16>(hints, "urgency").map(i64::from))
    .or_else(|| hint::<u16>(hints, "urgency").map(i64::from))
    .or_else(|| hint::<i64>(hints, "urgency"))
    .or_else(|| hint::<u64>(hints, "urgency").and_then(|u| i64::try_from(u).ok()))
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
    // only an id this server handed out is replaced, any other one gets a new id
    let issued = |id: u32| id != 0 && id < self.next_id.load(Ordering::Relaxed);
    let id = match replaces_id {
      id if issued(id) => id,
      _ => self.fresh_id(),
    };
    let urgency = match urgency(&hints) {
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
  use zbus::zvariant::Value;

  use super::*;

  fn server(next_id: u32) -> (Server, flume::Receiver<Event>) {
    let (events, received) = flume::unbounded();
    (
      Server {
        events,
        next_id: AtomicU32::new(next_id),
      },
      received,
    )
  }

  fn hints(pairs: Vec<(&str, Value<'_>)>) -> HashMap<String, OwnedValue> {
    pairs
      .into_iter()
      .map(|(k, v)| (k.to_string(), v.try_to_owned().unwrap()))
      .collect()
  }

  fn notify(server: &Server, replaces_id: u32, hints: HashMap<String, OwnedValue>) -> u32 {
    server.notify(
      "app".into(),
      replaces_id,
      "icon".into(),
      "summary".into(),
      "body".into(),
      vec!["default".into(), "Open".into()],
      hints,
      -1,
    )
  }

  fn received(rx: &flume::Receiver<Event>) -> Notification {
    match rx.try_recv().unwrap() {
      Event::Notify(n) => n,
      Event::Close(id) => panic!("closed {id}"),
    }
  }

  #[test]
  fn ids_count_up_unless_replacing() {
    let (server, rx) = server(1);
    assert_eq!(notify(&server, 0, HashMap::new()), 1);
    assert_eq!(notify(&server, 0, HashMap::new()), 2);
    assert_eq!(notify(&server, 1, HashMap::new()), 1);
    assert_eq!(notify(&server, 0, HashMap::new()), 3);
    assert_eq!(rx.len(), 4);
    let first = received(&rx);
    assert_eq!(
      (
        first.app_name.as_str(),
        first.app_icon.as_str(),
        first.summary.as_str(),
        first.body.as_str()
      ),
      ("app", "icon", "summary", "body")
    );
    assert_eq!(first.actions[0].label, "Open");
    assert!(!first.read && !first.resident);
  }

  #[test]
  fn hint_values() {
    let (server, rx) = server(1);
    for (value, urgency) in [
      (0u8, Urgency::Low),
      (1, Urgency::Normal),
      (2, Urgency::Critical),
      (7, Urgency::Normal),
    ] {
      notify(&server, 0, hints(vec![("urgency", value.into())]));
      assert_eq!(received(&rx).urgency, urgency);
    }
    notify(
      &server,
      0,
      hints(vec![
        ("desktop-entry", "firefox".into()),
        ("resident", true.into()),
      ]),
    );
    let n = received(&rx);
    assert_eq!(
      (n.desktop_entry.as_deref(), n.resident),
      (Some("firefox"), true)
    );
    // mistyped hints are ignored
    notify(
      &server,
      0,
      hints(vec![
        ("desktop-entry", 5u32.into()),
        ("resident", "yes".into()),
      ]),
    );
    let n = received(&rx);
    assert_eq!((n.desktop_entry, n.resident), (None, false));
  }

  #[test]
  fn urgency_accepts_any_integer() {
    let (server, rx) = server(1);
    let values: Vec<Value<'_>> = vec![
      2i32.into(),
      2u32.into(),
      2i16.into(),
      2u16.into(),
      2i64.into(),
      2u64.into(),
    ];
    for value in values {
      notify(&server, 0, hints(vec![("urgency", value)]));
      assert_eq!(received(&rx).urgency, Urgency::Critical);
    }
    notify(&server, 0, hints(vec![("urgency", 0i32.into())]));
    assert_eq!(received(&rx).urgency, Urgency::Low);
    notify(&server, 0, hints(vec![("urgency", "2".into())]));
    assert_eq!(received(&rx).urgency, Urgency::Normal);
  }

  #[test]
  fn unknown_replace_ids_get_a_fresh_id() {
    let (server, _rx) = server(1);
    let foreign = notify(&server, 2, HashMap::new());
    let fresh = notify(&server, 0, HashMap::new());
    let next = notify(&server, 0, HashMap::new());
    assert!(
      foreign != fresh && foreign != next,
      "ids {foreign} {fresh} {next}"
    );
  }

  #[test]
  fn ids_never_wrap_to_zero() {
    let (server, _rx) = server(u32::MAX);
    assert_eq!(notify(&server, 0, HashMap::new()), u32::MAX);
    assert_eq!(notify(&server, 0, HashMap::new()), 1);
    assert_eq!(notify(&server, 0, HashMap::new()), 2);
  }

  #[test]
  fn server_information() {
    let (server, _) = server(1);
    assert_eq!(
      server.get_capabilities(),
      ["body", "actions", "persistence"]
    );
    let (name, vendor, version, spec) = server.get_server_information();
    assert_eq!((name, vendor, spec), ("corona", "corona", "1.2"));
    assert_eq!(version, env!("CARGO_PKG_VERSION"));
  }

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
