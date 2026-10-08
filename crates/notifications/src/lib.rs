use std::{collections::HashMap, sync::atomic::AtomicU32};

use anyhow::Result;
use futures_lite::StreamExt;
use gpui_kit::{App, AppContext, Entity, Global};
use zbus::{
  Connection,
  fdo::{DBusProxy, RequestNameFlags, RequestNameReply},
  object_server::SignalEmitter,
  zvariant::Value,
};

use crate::server::{CloseReason, Event, NAME, PATH, Server};

pub use crate::state::{Action, Notification, Urgency};

mod server;
mod state;

#[derive(Clone)]
pub struct Notifications {
  pub notifications: Entity<Vec<Notification>>,
  pub do_not_disturb: Entity<bool>,
  pub active: Entity<bool>,
  conn: Connection,
}

impl Global for Notifications {}

pub struct Filter(pub Box<dyn Fn(&Notification) -> bool>);

impl Global for Filter {}

pub trait NotificationsExt {
  fn notifications(&self) -> &Notifications;
}

impl NotificationsExt for App {
  fn notifications(&self) -> &Notifications {
    self.global::<Notifications>()
  }
}

impl Notifications {
  pub fn list<'c>(&self, cx: &'c App) -> &'c [Notification] {
    self.notifications.read(cx)
  }

  pub fn do_not_disturb(&self, cx: &App) -> bool {
    *self.do_not_disturb.read(cx)
  }

  pub fn active(&self, cx: &App) -> bool {
    *self.active.read(cx)
  }

  pub fn has_unread(&self, cx: &App) -> bool {
    self.list(cx).iter().any(|n| !n.read)
  }

  pub fn mark_all_read(&self, cx: &mut App) {
    self.notifications.update(cx, |list, cx| {
      if state::mark_read(list) {
        cx.notify();
      }
    });
  }

  pub fn mark_read(&self, id: u32, cx: &mut App) {
    self.notifications.update(cx, |list, cx| {
      if let Some(n) = list.iter_mut().find(|n| n.id == id && !n.read) {
        n.read = true;
        cx.notify();
      }
    });
  }

  pub fn set_do_not_disturb(&self, enabled: bool, cx: &mut App) {
    self.do_not_disturb.write(cx, enabled);
  }

  pub fn dismiss(&self, id: u32, cx: &mut App) {
    self.remove(cx, |n| n.id == id);
  }

  pub fn clear_all(&self, cx: &mut App) {
    self.remove(cx, |_| true);
  }

  /// Goes through `org.freedesktop.Notifications` like any app's, so it lands
  /// wherever notifications go now, corona or another daemon.
  pub fn send(&self, summary: String, body: String) -> impl Future<Output = Result<()>> + use<> {
    let conn = self.conn.clone();
    async move {
      let hints: HashMap<&str, Value> = HashMap::new();
      conn
        .call_method(
          Some(NAME),
          PATH,
          Some(NAME),
          "Notify",
          &(
            "corona",
            0u32,
            "",
            summary,
            body,
            Vec::<&str>::new(),
            hints,
            -1i32,
          ),
        )
        .await?;
      Ok(())
    }
  }

  pub fn invoke_action(&self, id: u32, key: &str, cx: &mut App) {
    let Some(notification) = self.list(cx).iter().find(|n| n.id == id) else {
      return;
    };
    let resident = notification.resident;
    // apps may tear down on NotificationClosed: the action has to arrive first, so both go
    // out from one task
    let closed = match resident {
      true => Vec::new(),
      false => self.take(cx, |n| n.id == id),
    };
    let (conn, key) = (self.conn.clone(), key.to_string());
    cx.background_spawn(async move {
      let emitter = SignalEmitter::new(&conn, PATH)?;
      Server::action_invoked(&emitter, id, &key).await?;
      closed_signals(&emitter, closed).await
    })
    .detach();
  }

  fn remove(&self, cx: &mut App, matches: impl Fn(&Notification) -> bool) {
    let removed = self.take(cx, matches);
    if removed.is_empty() {
      return;
    }
    let conn = self.conn.clone();
    cx.background_spawn(async move {
      let emitter = SignalEmitter::new(&conn, PATH)?;
      closed_signals(&emitter, removed).await
    })
    .detach();
  }

  /// removes the matching notifications, their ids
  fn take(&self, cx: &mut App, matches: impl Fn(&Notification) -> bool) -> Vec<u32> {
    self.notifications.update(cx, |list, cx| {
      let removed: Vec<u32> = list.iter().filter(|n| matches(n)).map(|n| n.id).collect();
      if !removed.is_empty() {
        list.retain(|n| !matches(n));
        cx.notify();
      }
      removed
    })
  }
}

async fn closed_signals(emitter: &SignalEmitter<'_>, ids: Vec<u32>) -> zbus::Result<()> {
  for id in ids {
    Server::notification_closed(emitter, id, CloseReason::Dismissed as u32).await?;
  }
  Ok(())
}

/// `serve`: be the notification daemon. Without it the state stays empty, for
/// when another daemon is wanted.
pub async fn init(cx: &mut App, conn: &Connection, serve: bool) -> Result<()> {
  let (events_tx, events) = flume::unbounded();
  let dbus = DBusProxy::new(conn).await?;
  let mut acquired = dbus.receive_name_acquired().await?;
  let mut lost = dbus.receive_name_lost().await?;
  let mut owner = false;
  if serve {
    conn
      .object_server()
      .at(
        PATH,
        Server {
          events: events_tx,
          next_id: AtomicU32::new(1),
        },
      )
      .await?;
    let reply = conn
      .request_name_with_flags(NAME, RequestNameFlags::AllowReplacement.into())
      .await?;
    owner = matches!(
      reply,
      RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner
    );
    if !owner {
      tracing::info!("another notification daemon runs, corona takes over when it exits");
    }
  }

  let state = Notifications {
    notifications: cx.new(|_| Vec::new()),
    do_not_disturb: cx.new(|_| false),
    active: cx.new(|_| owner),
    conn: conn.clone(),
  };

  let notifications = state.notifications.clone();
  let conn_events = conn.clone();
  cx.spawn(async move |cx| {
    while let Ok(event) = events.recv_async().await {
      let dropped = notifications.update(cx, |list, cx| {
        match event {
          Event::Notify(n) if cx.try_global::<Filter>().is_some_and(|f| (f.0)(&n)) => {
            return Some(n.id);
          }
          Event::Notify(notification) => state::insert(list, notification),
          Event::Close(id) => list.retain(|n| n.id != id),
        }
        cx.notify();
        None
      });
      if let Some(id) = dropped
        && let Ok(emitter) = SignalEmitter::new(&conn_events, PATH)
      {
        let _ = closed_signals(&emitter, vec![id]).await;
      }
    }
  })
  .detach();

  let active = state.active.clone();
  cx.spawn(async move |cx| {
    loop {
      let owned = futures_lite::future::or(
        async {
          acquired
            .next()
            .await
            .map(|s| (true, s.args().ok().map(|a| a.name.to_string())))
        },
        async {
          lost
            .next()
            .await
            .map(|s| (false, s.args().ok().map(|a| a.name.to_string())))
        },
      )
      .await;
      let Some((owned, name)) = owned else {
        break;
      };
      if name.as_deref() == Some(NAME) {
        active.write(cx, owned);
      }
    }
  })
  .detach();

  cx.set_global(state);
  Ok(())
}

#[cfg(test)]
mod tests {
  use corona_utils::test_bus::{TestBus, settle, wait_until};
  use futures_lite::future::block_on;
  use gpui_kit::{self as gpui, TestAppContext};
  use zbus::{MatchRule, MessageStream, message::Type};

  use super::*;

  struct Running {
    _bus: TestBus,
    /// an app sending notifications
    client: Connection,
    signals: MessageStream,
  }

  fn start(cx: &mut TestAppContext, serve: bool) -> Running {
    cx.executor().allow_parking();
    let bus = TestBus::new();
    let (conn, client) = block_on(async { (bus.conn().await, bus.conn().await) });
    let rule = MatchRule::builder()
      .msg_type(Type::Signal)
      .interface(NAME)
      .unwrap()
      .build();
    let signals = block_on(MessageStream::for_match_rule(rule, &client, None)).unwrap();
    cx.update(|cx| {
      cx.foreground_executor()
        .clone()
        .block_on(init(cx, &conn, serve))
        .unwrap()
    });
    Running {
      _bus: bus,
      client,
      signals,
    }
  }

  fn notify(
    client: &Connection,
    summary: &str,
    replaces: u32,
    actions: &[&str],
    hints: HashMap<&str, Value<'_>>,
  ) -> u32 {
    let reply = block_on(client.call_method(
      Some(NAME),
      PATH,
      Some(NAME),
      "Notify",
      &(
        "app",
        replaces,
        "",
        summary,
        "body",
        actions.to_vec(),
        hints,
        -1i32,
      ),
    ))
    .unwrap();
    reply.body().deserialize().unwrap()
  }

  /// the next signal: its member, id and second argument. The shell sends
  /// signals from background tasks, so the app runs while waiting.
  fn next_signal(cx: &mut TestAppContext, running: &mut Running) -> (String, u32, String) {
    let mut message = None;
    wait_until(cx, |_| {
      message = block_on(futures_lite::future::poll_once(running.signals.next())).flatten();
      message.is_some()
    });
    let message = message.unwrap().unwrap();
    let member = message.header().member().unwrap().to_string();
    let body = message.body();
    if member == "NotificationClosed" {
      let (id, reason): (u32, u32) = body.deserialize().unwrap();
      return (member, id, reason.to_string());
    }
    let (id, key): (u32, String) = body.deserialize().unwrap();
    (member, id, key)
  }

  fn ids(cx: &mut TestAppContext) -> Vec<u32> {
    cx.read(|cx| cx.notifications().list(cx).iter().map(|n| n.id).collect())
  }

  #[gpui::test]
  fn serves_notifications(cx: &mut TestAppContext) {
    let running = start(cx, true);
    cx.read(|cx| assert!(cx.notifications().active(cx)));
    let first = notify(&running.client, "one", 0, &[], HashMap::new());
    let second = notify(&running.client, "two", 0, &[], HashMap::new());
    wait_until(cx, |cx| ids(cx) == [second, first]);
    // a replacement moves to the front and is unread
    cx.update(|cx| cx.notifications().clone().mark_all_read(cx));
    cx.read(|cx| assert!(!cx.notifications().has_unread(cx)));
    notify(&running.client, "one again", first, &[], HashMap::new());
    wait_until(cx, |cx| ids(cx) == [first, second]);
    cx.read(|cx| {
      let notifications = cx.notifications();
      assert!(notifications.has_unread(cx));
      assert_eq!(notifications.list(cx)[0].summary, "one again");
    });
  }

  #[gpui::test]
  fn read_state(cx: &mut TestAppContext) {
    let running = start(cx, true);
    let a = notify(&running.client, "a", 0, &[], HashMap::new());
    let b = notify(&running.client, "b", 0, &[], HashMap::new());
    wait_until(cx, |cx| ids(cx).len() == 2);
    cx.update(|cx| cx.notifications().clone().mark_read(a, cx));
    cx.read(|cx| {
      let list = cx.notifications().list(cx);
      assert!(list.iter().find(|n| n.id == a).unwrap().read);
      assert!(!list.iter().find(|n| n.id == b).unwrap().read);
    });
    // unknown ids and repeats are fine
    cx.update(|cx| {
      cx.notifications().clone().mark_read(999, cx);
      cx.notifications().clone().mark_read(b, cx);
      cx.notifications().clone().mark_read(b, cx);
    });
    cx.read(|cx| assert!(!cx.notifications().has_unread(cx)));

    cx.update(|cx| cx.notifications().clone().set_do_not_disturb(true, cx));
    cx.read(|cx| assert!(cx.notifications().do_not_disturb(cx)));
  }

  #[gpui::test]
  fn filtered_notifications_are_dropped(cx: &mut TestAppContext) {
    let mut running = start(cx, true);
    cx.update(|cx| cx.set_global(Filter(Box::new(|n| n.summary == "spam"))));
    let spam = notify(&running.client, "spam", 0, &[], HashMap::new());
    assert_eq!(
      next_signal(cx, &mut running),
      ("NotificationClosed".into(), spam, "2".into())
    );
    let kept = notify(&running.client, "ham", 0, &[], HashMap::new());
    wait_until(cx, |cx| ids(cx) == [kept]);
  }

  #[gpui::test]
  fn apps_close_their_notifications(cx: &mut TestAppContext) {
    let mut running = start(cx, true);
    let id = notify(&running.client, "a", 0, &[], HashMap::new());
    wait_until(cx, |cx| ids(cx) == [id]);
    block_on(
      running
        .client
        .call_method(Some(NAME), PATH, Some(NAME), "CloseNotification", &(id,)),
    )
    .unwrap();
    wait_until(cx, |cx| ids(cx).is_empty());
    assert_eq!(
      next_signal(cx, &mut running),
      ("NotificationClosed".into(), id, "3".into())
    );
  }

  #[gpui::test]
  fn dismissing_tells_the_app(cx: &mut TestAppContext) {
    let mut running = start(cx, true);
    let a = notify(&running.client, "a", 0, &[], HashMap::new());
    let b = notify(&running.client, "b", 0, &[], HashMap::new());
    wait_until(cx, |cx| ids(cx).len() == 2);
    cx.update(|cx| cx.notifications().clone().dismiss(a, cx));
    assert_eq!(ids(cx), [b]);
    assert_eq!(
      next_signal(cx, &mut running),
      ("NotificationClosed".into(), a, "2".into())
    );
    cx.update(|cx| cx.notifications().clone().clear_all(cx));
    assert!(ids(cx).is_empty());
    assert_eq!(
      next_signal(cx, &mut running),
      ("NotificationClosed".into(), b, "2".into())
    );
  }

  #[gpui::test]
  fn actions(cx: &mut TestAppContext) {
    let mut running = start(cx, true);
    let plain = notify(
      &running.client,
      "a",
      0,
      &["default", "Open"],
      HashMap::new(),
    );
    let resident = notify(
      &running.client,
      "b",
      0,
      &["play", "Play"],
      HashMap::from([("resident", Value::from(true))]),
    );
    wait_until(cx, |cx| ids(cx).len() == 2);

    cx.update(|cx| {
      cx.notifications()
        .clone()
        .invoke_action(resident, "play", cx)
    });
    assert_eq!(
      next_signal(cx, &mut running),
      ("ActionInvoked".into(), resident, "play".into())
    );
    assert_eq!(ids(cx).len(), 2);

    cx.update(|cx| {
      cx.notifications()
        .clone()
        .invoke_action(plain, "default", cx)
    });
    let mut signals = vec![next_signal(cx, &mut running), next_signal(cx, &mut running)];
    signals.sort();
    assert_eq!(
      signals,
      [
        ("ActionInvoked".into(), plain, "default".into()),
        ("NotificationClosed".into(), plain, "2".into()),
      ]
    );
    assert_eq!(ids(cx), [resident]);

    // an unknown id does nothing
    cx.update(|cx| cx.notifications().clone().invoke_action(999, "default", cx));
    settle(cx);
    assert_eq!(ids(cx), [resident]);
  }

  #[gpui::test]
  fn action_comes_before_close(cx: &mut TestAppContext) {
    let mut running = start(cx, true);
    let plain = notify(
      &running.client,
      "a",
      0,
      &["default", "Open"],
      HashMap::new(),
    );
    wait_until(cx, |cx| ids(cx).len() == 1);
    cx.update(|cx| {
      cx.notifications()
        .clone()
        .invoke_action(plain, "default", cx)
    });
    assert_eq!(next_signal(cx, &mut running).0, "ActionInvoked");
  }

  #[gpui::test]
  fn send_goes_to_the_daemon(cx: &mut TestAppContext) {
    let _running = start(cx, true);
    let task = cx.read(|cx| cx.notifications().send("Hi".into(), "there".into()));
    block_on(task).unwrap();
    wait_until(cx, |cx| ids(cx).len() == 1);
    cx.read(|cx| {
      let n = &cx.notifications().list(cx)[0];
      assert_eq!(
        (n.app_name.as_str(), n.summary.as_str(), n.body.as_str()),
        ("corona", "Hi", "there")
      );
    });
  }

  #[gpui::test]
  fn without_serving(cx: &mut TestAppContext) {
    let running = start(cx, false);
    cx.read(|cx| assert!(!cx.notifications().active(cx)));
    // nobody answers
    let task = cx.read(|cx| cx.notifications().send("Hi".into(), "there".into()));
    assert!(block_on(task).is_err());
    drop(running);
  }

  #[gpui::test]
  fn takes_over_from_another_daemon(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let bus = TestBus::new();
    let other = block_on(async {
      let other = bus.conn().await;
      other.request_name(NAME).await.unwrap();
      other
    });
    let conn = block_on(bus.conn());
    cx.update(|cx| {
      cx.foreground_executor()
        .clone()
        .block_on(init(cx, &conn, true))
        .unwrap()
    });
    cx.read(|cx| assert!(!cx.notifications().active(cx)));
    // queued: corona gets the name once the other daemon leaves
    drop(other);
    wait_until(cx, |cx| cx.read(|cx| cx.notifications().active(cx)));
    // and gives it up to one that replaces it
    let replacing = block_on(async {
      let replacing = bus.conn().await;
      replacing
        .request_name_with_flags(NAME, RequestNameFlags::ReplaceExisting.into())
        .await
        .unwrap();
      replacing
    });
    wait_until(cx, |cx| !cx.read(|cx| cx.notifications().active(cx)));
    drop(replacing);
  }

  #[gpui::test]
  fn close_nonexistent_or_invalid_id(cx: &mut TestAppContext) {
    let mut running = start(cx, true);
    let id = notify(&running.client, "real", 0, &[], HashMap::new());
    wait_until(cx, |cx| ids(cx) == [id]);

    // Close invalid ID 0
    block_on(running.client.call_method(
      Some(NAME),
      PATH,
      Some(NAME),
      "CloseNotification",
      &(0u32,),
    ))
    .unwrap();
    assert_eq!(
      next_signal(cx, &mut running),
      ("NotificationClosed".into(), 0, "3".into())
    );
    // Real notification still intact
    assert_eq!(ids(cx), [id]);

    // Close non-existent ID 9999
    block_on(running.client.call_method(
      Some(NAME),
      PATH,
      Some(NAME),
      "CloseNotification",
      &(9999u32,),
    ))
    .unwrap();
    assert_eq!(
      next_signal(cx, &mut running),
      ("NotificationClosed".into(), 9999, "3".into())
    );
    assert_eq!(ids(cx), [id]);

    // Close real ID
    block_on(
      running
        .client
        .call_method(Some(NAME), PATH, Some(NAME), "CloseNotification", &(id,)),
    )
    .unwrap();
    assert_eq!(
      next_signal(cx, &mut running),
      ("NotificationClosed".into(), id, "3".into())
    );
    wait_until(cx, |cx| ids(cx).is_empty());

    // Close already-closed ID
    block_on(
      running
        .client
        .call_method(Some(NAME), PATH, Some(NAME), "CloseNotification", &(id,)),
    )
    .unwrap();
    assert_eq!(
      next_signal(cx, &mut running),
      ("NotificationClosed".into(), id, "3".into())
    );
    assert!(ids(cx).is_empty());
  }

  #[gpui::test]
  fn invoke_nonexistent_action_key(cx: &mut TestAppContext) {
    let mut running = start(cx, true);
    let plain = notify(
      &running.client,
      "plain",
      0,
      &["default", "Open"],
      HashMap::new(),
    );
    let resident = notify(
      &running.client,
      "resident",
      0,
      &["play", "Play"],
      HashMap::from([("resident", Value::from(true))]),
    );
    wait_until(cx, |cx| ids(cx).len() == 2);

    // Invoke unknown action on resident notification
    cx.update(|cx| {
      cx.notifications()
        .clone()
        .invoke_action(resident, "nonexistent", cx)
    });
    assert_eq!(
      next_signal(cx, &mut running),
      ("ActionInvoked".into(), resident, "nonexistent".into())
    );
    // Resident notification is kept
    assert_eq!(ids(cx).len(), 2);

    // Invoke unknown action on non-resident notification
    cx.update(|cx| {
      cx.notifications()
        .clone()
        .invoke_action(plain, "nonexistent", cx)
    });
    let mut signals = vec![next_signal(cx, &mut running), next_signal(cx, &mut running)];
    signals.sort();
    assert_eq!(
      signals,
      [
        ("ActionInvoked".into(), plain, "nonexistent".into()),
        ("NotificationClosed".into(), plain, "2".into()),
      ]
    );
    // Non-resident notification is removed
    assert_eq!(ids(cx), [resident]);
  }

  #[gpui::test]
  fn replaces_closed_notification_in_ui_state(cx: &mut TestAppContext) {
    let mut running = start(cx, true);
    let id = notify(&running.client, "initial", 0, &[], HashMap::new());
    wait_until(cx, |cx| ids(cx) == [id]);

    // App closes it
    block_on(
      running
        .client
        .call_method(Some(NAME), PATH, Some(NAME), "CloseNotification", &(id,)),
    )
    .unwrap();
    wait_until(cx, |cx| ids(cx).is_empty());
    assert_eq!(
      next_signal(cx, &mut running),
      ("NotificationClosed".into(), id, "3".into())
    );

    // App replaces the previously closed ID
    let rep_id = notify(&running.client, "resurrected", id, &[], HashMap::new());
    assert_eq!(rep_id, id);
    wait_until(cx, |cx| ids(cx) == [id]);
    cx.read(|cx| {
      assert_eq!(cx.notifications().list(cx)[0].summary, "resurrected");
    });
  }

  #[gpui::test]
  fn expire_timeout_retains_notification(cx: &mut TestAppContext) {
    let running = start(cx, true);
    for timeout in [-1i32, 0, 500, 10000] {
      let reply = block_on(running.client.call_method(
        Some(NAME),
        PATH,
        Some(NAME),
        "Notify",
        &(
          "app",
          0u32,
          "",
          format!("timeout {timeout}"),
          "body",
          Vec::<&str>::new(),
          HashMap::<&str, Value<'_>>::new(),
          timeout,
        ),
      ))
      .unwrap();
      let id: u32 = reply.body().deserialize().unwrap();
      assert!(id > 0);
    }
    wait_until(cx, |cx| ids(cx).len() == 4);
  }
}
