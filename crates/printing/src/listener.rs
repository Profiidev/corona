use std::{cell::Cell, path::Path, rc::Rc, sync::Arc, time::Duration};

use anyhow::Result;
use corona_utils::{entity::WriteChangedExt, error::ErrorLogExt, stream::pump};
use futures_lite::{Stream, StreamExt, future::poll_once};
use gpui_kit::{App, Entity};
use zbus::{Connection, MatchRule, Message, MessageStream, message::Type};

use crate::{
  Printing,
  avahi::{self, AVAHI, Found, Item, ServerProxy},
  cups,
  ipp::IppError,
  state::{NetworkPrinter, Scanner},
};

const NOTIFIER: &str = "org.cups.cupsd.Notifier";
/// renew well before the lease runs out
const RENEW: Duration = Duration::from_secs(cups::LEASE_SECONDS as u64 / 2);
// shortcut: cupsd has no bus name to watch, so while it is unreachable the
// subscription is retried on a timer. Upgrade if cupsd ever claims a name.
const RETRY: Duration = Duration::from_secs(60);

fn owner_changed(name: &str) -> Result<MatchRule<'static>> {
  Ok(
    MatchRule::builder()
      .msg_type(Type::Signal)
      .sender("org.freedesktop.DBus")?
      .interface("org.freedesktop.DBus")?
      .member("NameOwnerChanged")?
      .arg(0, name.to_string())?
      .build(),
  )
}

/// Fires on the CUPS notifier's signals. The notifier is a short-lived helper
/// cupsd starts per event, so match the interface, not a sender.
pub async fn subscribe(conn: &Connection) -> Result<impl Stream<Item = ()> + Unpin + use<>> {
  let rule = MatchRule::builder()
    .msg_type(Type::Signal)
    .interface(NOTIFIER)?
    .build();
  Ok(
    MessageStream::for_match_rule(rule, conn, None)
      .await?
      .map(|_| ()),
  )
}

/// Re-reads printers and jobs on every change burst and on `pokes`.
pub fn listener(
  cx: &mut App,
  sock: Arc<Path>,
  changes: impl Stream<Item = ()> + Unpin + 'static,
  pokes: flume::Receiver<()>,
  state: Printing,
) {
  let mut changes = pump(cx, changes).or(pokes.into_stream());
  cx.spawn(async move |cx| {
    loop {
      let printers = cups::printers(&sock).await.log_err().unwrap_or_default();
      let jobs = match printers.is_empty() {
        true => Vec::new(),
        false => cups::jobs(&sock).await.log_err().unwrap_or_default(),
      };
      state.printers.write_changed(cx, printers);
      state.jobs.write_changed(cx, jobs);

      if changes.next().await.is_none() {
        break;
      }
      // drain the rest of the burst already queued, one snapshot covers it
      while let Some(Some(_)) = poll_once(changes.next()).await {}
    }
  })
  .detach();
}

/// Keeps a CUPS event subscription alive so the notifier signals at all.
pub fn subscription(cx: &mut App, sock: Arc<Path>, poke: flume::Sender<()>) {
  let current = Rc::new(Cell::new(None));
  let id = current.clone();
  let quit_sock = sock.clone();
  cx.on_app_quit(move |_| {
    let (sock, id) = (quit_sock.clone(), current.take());
    async move {
      if let Some(id) = id {
        cups::unsubscribe(&sock, id).await.log_err().ok();
      }
    }
  })
  .detach();

  cx.spawn(async move |cx| {
    loop {
      let wait = match id.get() {
        Some(old) => match cups::renew(&sock, old).await {
          Ok(()) => RENEW,
          Err(_) => {
            // expired or cupsd restarted without it, make a new one
            id.set(None);
            continue;
          }
        },
        None => match cups::subscribe(&sock).await {
          Ok(new) => {
            id.set(Some(new));
            // cupsd may have come back, read what changed while it was gone
            poke.send(()).ok();
            RENEW
          }
          Err(e) if e.downcast_ref::<IppError>().is_some() => {
            tracing::warn!("CUPS refused the event subscription, printers update on open: {e}");
            return;
          }
          Err(_) => RETRY,
        },
      };
      cx.background_executor().timer(wait).await;
    }
  })
  .detach();
}

/// Fires on every Avahi service browser signal and on avahi-daemon starting
/// or quitting.
pub async fn subscribe_avahi(
  conn: &Connection,
) -> Result<impl Stream<Item = Message> + Unpin + use<>> {
  let browser = MatchRule::builder()
    .msg_type(Type::Signal)
    .sender(AVAHI)?
    .interface("org.freedesktop.Avahi.ServiceBrowser")?
    .build();
  let browser = MessageStream::for_match_rule(browser, conn, None).await?;
  let owner = MessageStream::for_match_rule(owner_changed(AVAHI)?, conn, None).await?;
  Ok(browser.or(owner).filter_map(|m| m.ok()))
}

/// Follows Avahi's announcements of scanners and of network printers.
pub fn avahi_listener(
  cx: &mut App,
  conn: Connection,
  messages: impl Stream<Item = Message> + Unpin + 'static,
  scanners: Entity<Vec<Scanner>>,
  printers: Entity<Vec<NetworkPrinter>>,
) {
  let mut messages = pump(cx, messages);
  cx.spawn(async move |cx| {
    let Ok(server) = ServerProxy::new(&conn).await.log_err() else {
      return;
    };
    // fails when avahi-daemon is not running, NameOwnerChanged retries
    avahi::browse(&server).await.ok();
    let mut found = Found::new();
    while let Some(message) = messages.next().await {
      let header = message.header();
      let member = header.member().map(|m| m.as_str()).unwrap_or_default();
      let body = message.body();
      match member {
        "ItemNew" => {
          let Ok(item) = body.deserialize::<Item>() else {
            continue;
          };
          if let Ok(service) = avahi::resolve(&server, &item).await.log_err() {
            found.insert(avahi::key(&item), service);
          }
        }
        "ItemRemove" => {
          if let Ok(item) = body.deserialize::<Item>() {
            found.remove(&avahi::key(&item));
          }
        }
        "NameOwnerChanged" => {
          found.clear();
          let Ok((_, _, owner)) = body.deserialize::<(String, String, String)>() else {
            continue;
          };
          if !owner.is_empty() {
            avahi::browse(&server).await.log_err().ok();
          }
        }
        _ => continue,
      }
      scanners.write_changed(cx, avahi::scanners(&found));
      printers.write_changed(cx, avahi::printers(&found));
    }
  })
  .detach();
}
