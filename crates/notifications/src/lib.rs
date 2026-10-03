use std::sync::atomic::AtomicU32;

use anyhow::Result;
use futures_lite::StreamExt;
use gpui_kit::{App, AppContext, Entity, Global};
use zbus::{
  Connection,
  fdo::{DBusProxy, RequestNameFlags, RequestNameReply},
  object_server::SignalEmitter,
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

  pub fn set_do_not_disturb(&self, enabled: bool, cx: &mut App) {
    self.do_not_disturb.write(cx, enabled);
  }

  pub fn dismiss(&self, id: u32, cx: &mut App) {
    self.remove(cx, |n| n.id == id);
  }

  pub fn clear_all(&self, cx: &mut App) {
    self.remove(cx, |_| true);
  }

  pub fn invoke_action(&self, id: u32, key: &str, cx: &mut App) {
    let Some(notification) = self.list(cx).iter().find(|n| n.id == id) else {
      return;
    };
    let resident = notification.resident;
    let (conn, key) = (self.conn.clone(), key.to_string());
    cx.background_spawn(async move {
      let emitter = SignalEmitter::new(&conn, PATH)?;
      Server::action_invoked(&emitter, id, &key).await
    })
    .detach();
    if !resident {
      self.dismiss(id, cx);
    }
  }

  fn remove(&self, cx: &mut App, matches: impl Fn(&Notification) -> bool) {
    let removed: Vec<u32> = self.notifications.update(cx, |list, cx| {
      let removed = list.iter().filter(|n| matches(n)).map(|n| n.id).collect();
      list.retain(|n| !matches(n));
      cx.notify();
      removed
    });
    let conn = self.conn.clone();
    cx.background_spawn(async move {
      let emitter = SignalEmitter::new(&conn, PATH)?;
      for id in removed {
        Server::notification_closed(&emitter, id, CloseReason::Dismissed as u32).await?;
      }
      zbus::Result::Ok(())
    })
    .detach();
  }
}

pub async fn init(cx: &mut App, conn: &Connection) -> Result<()> {
  let (events_tx, events) = flume::unbounded();
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

  let dbus = DBusProxy::new(conn).await?;
  let mut acquired = dbus.receive_name_acquired().await?;
  let mut lost = dbus.receive_name_lost().await?;
  let reply = conn
    .request_name_with_flags(NAME, RequestNameFlags::AllowReplacement.into())
    .await?;
  let owner = matches!(
    reply,
    RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner
  );
  if !owner {
    tracing::info!("another notification daemon runs, corona takes over when it exits");
  }

  let state = Notifications {
    notifications: cx.new(|_| Vec::new()),
    do_not_disturb: cx.new(|_| false),
    active: cx.new(|_| owner),
    conn: conn.clone(),
  };

  let notifications = state.notifications.clone();
  cx.spawn(async move |cx| {
    while let Ok(event) = events.recv_async().await {
      notifications.update(cx, |list, cx| {
        match event {
          Event::Notify(notification) => state::insert(list, notification),
          Event::Close(id) => list.retain(|n| n.id != id),
        }
        cx.notify();
      });
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
