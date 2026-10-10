//! `org.freedesktop.ScreenSaver`, the D-Bus way to inhibit idling: games
//! through SDL2 or Steam, browsers. Controller input never reaches the
//! compositor, so without this a game is idle after the first timeout.

use std::{
  collections::HashMap,
  sync::{Arc, Mutex},
};

use anyhow::{Result, bail};
use futures_lite::StreamExt;
use zbus::{
  Connection,
  fdo::{DBusProxy, RequestNameFlags, RequestNameReply},
  interface,
  message::Header,
};

const NAME: &str = "org.freedesktop.ScreenSaver";
/// SDL and browsers use the first, older apps the second
const PATHS: [&str; 2] = ["/org/freedesktop/ScreenSaver", "/ScreenSaver"];

#[derive(Default)]
struct Cookies {
  last: u32,
  /// the bus name that asked, by cookie
  owners: HashMap<u32, String>,
}

#[derive(Clone)]
struct ScreenSaver {
  cookies: Arc<Mutex<Cookies>>,
  inhibited: flume::Sender<bool>,
}

impl ScreenSaver {
  /// Reports when inhibited flips
  fn change(&self, f: impl FnOnce(&mut Cookies)) {
    let mut cookies = self.cookies.lock().unwrap();
    let before = !cookies.owners.is_empty();
    f(&mut cookies);
    let after = !cookies.owners.is_empty();
    if before != after {
      let _ = self.inhibited.send(after);
    }
  }
}

#[interface(name = "org.freedesktop.ScreenSaver")]
impl ScreenSaver {
  fn inhibit(
    &self,
    #[zbus(header)] header: Header<'_>,
    application: String,
    reason: String,
  ) -> u32 {
    tracing::info!("{application} inhibits idling: {reason}");
    let owner = header.sender().map(|s| s.to_string()).unwrap_or_default();
    let mut cookie = 0;
    self.change(|c| {
      c.last = c.last.checked_add(1).unwrap_or(1);
      cookie = c.last;
      c.owners.insert(cookie, owner);
    });
    cookie
  }

  fn un_inhibit(&self, cookie: u32) {
    self.change(|c| {
      c.owners.remove(&cookie);
    });
  }
}

/// Serves on `conn` until it closes; `inhibited` hears each change
pub(crate) async fn serve(conn: Connection, inhibited: flume::Sender<bool>) -> Result<()> {
  let server = ScreenSaver {
    cookies: Default::default(),
    inhibited,
  };
  for path in PATHS {
    conn.object_server().at(path, server.clone()).await?;
  }
  let reply = conn
    .request_name_with_flags(NAME, RequestNameFlags::DoNotQueue.into())
    .await?;
  if !matches!(
    reply,
    RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner
  ) {
    bail!("another app serves {NAME}");
  }
  // apps that exit or crash without UnInhibit
  let mut owners = DBusProxy::new(&conn)
    .await?
    .receive_name_owner_changed()
    .await?;
  while let Some(signal) = owners.next().await {
    let Ok(args) = signal.args() else { continue };
    if args.new_owner().is_none() {
      let gone = args.name().to_string();
      server.change(|c| c.owners.retain(|_, owner| *owner != gone));
    }
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use std::{thread, time::Duration};

  use corona_utils::test_bus::TestBus;
  use futures_lite::future::block_on;

  use super::*;

  fn next(rx: &flume::Receiver<bool>) -> bool {
    rx.recv_timeout(Duration::from_secs(10)).unwrap()
  }

  #[test]
  fn inhibits_until_uninhibited_or_gone() {
    let bus = TestBus::new();
    let (tx, rx) = flume::unbounded();
    let conn = block_on(bus.conn());
    thread::spawn(move || block_on(serve(conn, tx)));

    let app = block_on(bus.conn());
    let inhibit = |conn: &Connection, path: &str| -> u32 {
      let reply = loop {
        match block_on(conn.call_method(
          Some(NAME),
          path,
          Some(NAME),
          "Inhibit",
          &("game", "playing"),
        )) {
          Ok(reply) => break reply,
          // the server is not up yet
          Err(_) => thread::sleep(Duration::from_millis(10)),
        }
      };
      reply.body().deserialize().unwrap()
    };

    let a = inhibit(&app, PATHS[0]);
    assert!(next(&rx));
    let b = inhibit(&app, PATHS[1]);
    assert_ne!(a, b);
    for cookie in [a, b] {
      block_on(app.call_method(Some(NAME), PATHS[0], Some(NAME), "UnInhibit", &cookie)).unwrap();
    }
    assert!(!next(&rx));

    // an app that leaves without UnInhibit
    inhibit(&app, PATHS[0]);
    assert!(next(&rx));
    drop(app);
    assert!(!next(&rx));
    assert!(rx.is_empty());
  }
}
