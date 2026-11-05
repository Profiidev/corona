use std::{
  collections::HashMap,
  pin::Pin,
  time::{Duration, Instant},
};

use anyhow::Result;
use corona_utils::{entity::WriteChangedExt, error::ErrorLogExt, stream::pump};
use futures_lite::{Stream, StreamExt, future::or};
use gpui_kit::App;
use smol::Timer;
use zbus::{Connection, MatchRule, MessageStream, message::Type};

use crate::{
  Tray,
  proxy::{ITEM_INTERFACE, MENU_INTERFACE, WATCHER_NAME, parse_address},
  snapshot::{Activatable, merge, prune_cache, refresh_items, snapshot},
  state::TrayItem,
};

const DEBOUNCE: Duration = Duration::from_millis(150);
/// Delays before re-reading items that failed their first read, then it waits for signals
const RETRY: [Duration; 2] = [Duration::from_secs(1), Duration::from_secs(3)];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
  /// Items registered or unregistered
  Watcher,
  /// A property of an item changed
  Item,
  /// A dbusmenu layout or property changed
  Menu,
}

/// A signal with who sent it and from which object
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
  pub sender: String,
  pub path: String,
  pub kind: Kind,
}

pub type Changes = Pin<Box<dyn Stream<Item = Change> + Send>>;

pub async fn subscribe(conn: &Connection) -> Result<Changes> {
  let signal = || MatchRule::builder().msg_type(Type::Signal);
  let rules = [
    (Kind::Watcher, signal().interface(WATCHER_NAME)?.build()),
    (Kind::Item, signal().interface(ITEM_INTERFACE)?.build()),
    (Kind::Menu, signal().interface(MENU_INTERFACE)?.build()),
    (
      Kind::Item,
      signal()
        .interface("org.freedesktop.DBus.Properties")?
        .member("PropertiesChanged")?
        .arg(0, ITEM_INTERFACE)?
        .build(),
    ),
  ];
  let mut changes: Changes = Box::pin(futures_lite::stream::empty());
  for (kind, rule) in rules {
    let signals = MessageStream::for_match_rule(rule, conn, None)
      .await?
      .filter_map(move |message| {
        let message = message.ok()?;
        let header = message.header();
        Some(Change {
          sender: header.sender()?.to_string(),
          path: header.path()?.to_string(),
          kind,
        })
      });
    changes = Box::pin(changes.or(signals));
  }
  Ok(changes)
}

/// Signals waiting out their debounce: one slot per item, so a noisy item delays no other, and
/// one for the full refresh
#[derive(Default)]
struct Pending {
  full: Option<Instant>,
  /// address -> (due, menu layout too)
  items: HashMap<String, (Instant, bool)>,
}

impl Pending {
  fn full(&mut self, due: Instant) {
    self.full = Some(self.full.map_or(due, |d| d.min(due)));
  }

  /// Queues what `change` touches; a sender or path no item has is ignored
  fn note(&mut self, items: &[TrayItem], change: &Change, now: Instant) {
    let due = now + DEBOUNCE;
    if change.kind == Kind::Watcher {
      return self.full(due);
    }
    let menu = change.kind == Kind::Menu;
    for item in items.iter().filter(|i| touches(i, change)) {
      let slot = self
        .items
        .entry(item.address.clone())
        .or_insert((due, false));
      slot.1 |= menu;
    }
  }

  fn next_due(&self) -> Option<Instant> {
    self
      .items
      .values()
      .map(|(due, _)| *due)
      .chain(self.full)
      .min()
  }

  /// What is due at `now`: whether to refresh everything, else the items; a full refresh covers
  /// every queued item
  fn take_due(&mut self, now: Instant) -> (bool, Vec<(String, bool)>) {
    if self.full.is_some_and(|d| d <= now) {
      self.full = None;
      self.items.clear();
      return (true, Vec::new());
    }
    let due: Vec<_> = self
      .items
      .iter()
      .filter(|(_, (d, _))| *d <= now)
      .map(|(a, (_, menu))| (a.clone(), *menu))
      .collect();
    for (address, _) in &due {
      self.items.remove(address);
    }
    (false, due)
  }
}

/// Whether `change` came from `item`
fn touches(item: &TrayItem, change: &Change) -> bool {
  let (bus, path) = parse_address(&item.address);
  bus == change.sender
    && match change.kind {
      Kind::Item => path == change.path,
      Kind::Menu => item
        .menu_path
        .as_ref()
        .is_some_and(|p| p.as_str() == change.path),
      Kind::Watcher => false,
    }
}

pub fn listener(cx: &mut App, conn: Connection, changes: Changes, state: Tray) {
  let mut changes = pump(cx, changes);
  cx.spawn(async move |cx| {
    let mut activatable = Activatable::new();
    let mut previous: Vec<TrayItem> = Vec::new();
    let mut retries = 0;
    let mut pending = Pending::default();
    pending.full(Instant::now());
    loop {
      let wait = match pending.next_due() {
        Some(due) => {
          or(async { changes.next().await.map(Some) }, async {
            Timer::at(due).await;
            Some(None)
          })
          .await
        }
        None => changes.next().await.map(Some),
      };
      match wait {
        None => break,
        Some(Some(change)) => {
          pending.note(&previous, &change, Instant::now());
          continue;
        }
        Some(None) => {}
      }

      let (full, targets) = pending.take_due(Instant::now());
      let items = if full {
        let Ok((items, dropped)) = snapshot(&conn, &mut activatable, &previous).await.log_err()
        else {
          continue;
        };
        activatable.retain(|address, _| items.iter().any(|i| &i.address == address));
        match dropped.then(|| RETRY.get(retries)).flatten() {
          Some(delay) => {
            retries += 1;
            pending.full(Instant::now() + *delay);
          }
          None if !dropped => retries = 0,
          None => {}
        }
        items
      } else {
        let refreshed = refresh_items(&conn, &mut activatable, &previous, &targets).await;
        let mut items = previous.clone();
        merge(&mut items, refreshed);
        items
      };
      let kept = items.clone();
      smol::unblock(move || prune_cache(&kept)).await;
      previous = items.clone();
      state.items.write_changed(cx, items);
    }
  })
  .detach();
}

#[cfg(test)]
mod tests {
  use super::*;

  fn item(address: &str, menu: Option<&str>) -> TrayItem {
    TrayItem {
      address: address.into(),
      id: String::new(),
      title: None,
      status: Default::default(),
      category: Default::default(),
      icon: None,
      icon_key: None,
      tooltip: None,
      item_is_menu: false,
      can_activate: true,
      menu: Vec::new(),
      menu_path: menu.map(|p| zbus::zvariant::OwnedObjectPath::try_from(p).unwrap()),
    }
  }

  fn change(sender: &str, path: &str, kind: Kind) -> Change {
    Change {
      sender: sender.into(),
      path: path.into(),
      kind,
    }
  }

  fn queued(p: &Pending) -> Vec<(String, bool)> {
    let mut v: Vec<_> = p.items.iter().map(|(a, (_, m))| (a.clone(), *m)).collect();
    v.sort();
    v
  }

  #[test]
  fn senders_resolve_to_their_items() {
    let items = [
      item(":1.1/StatusNotifierItem", Some("/Menu")),
      item(":1.1/Other", None),
      item(":1.2/StatusNotifierItem", None),
    ];
    let now = Instant::now();
    let mut p = Pending::default();
    p.note(&items, &change(":1.1", "/Other", Kind::Item), now);
    assert_eq!(queued(&p), [(":1.1/Other".to_string(), false)]);
    // a menu signal marks the layout, matched by the menu object
    p.note(&items, &change(":1.1", "/Menu", Kind::Menu), now);
    p.note(
      &items,
      &change(":1.1", "/StatusNotifierItem", Kind::Item),
      now,
    );
    assert_eq!(
      queued(&p),
      [
        (":1.1/Other".to_string(), false),
        (":1.1/StatusNotifierItem".to_string(), true)
      ]
    );
    // unknown senders and paths are ignored, and queue nothing
    let mut p = Pending::default();
    p.note(
      &items,
      &change(":1.9", "/StatusNotifierItem", Kind::Item),
      now,
    );
    p.note(&items, &change(":1.1", "/Nope", Kind::Item), now);
    p.note(&items, &change(":1.2", "/Menu", Kind::Menu), now);
    assert!(p.next_due().is_none());
  }

  #[test]
  fn watcher_signals_queue_a_full_refresh_and_debounce_per_item() {
    let items = [
      item(":1.1/StatusNotifierItem", None),
      item(":1.2/StatusNotifierItem", None),
    ];
    let t0 = Instant::now();
    let mut p = Pending::default();
    p.note(
      &items,
      &change(":1.1", "/StatusNotifierItem", Kind::Item),
      t0,
    );
    // a later signal does not push the item's due time back
    let t1 = t0 + DEBOUNCE / 2;
    p.note(
      &items,
      &change(":1.1", "/StatusNotifierItem", Kind::Item),
      t1,
    );
    p.note(
      &items,
      &change(":1.2", "/StatusNotifierItem", Kind::Item),
      t1,
    );
    assert_eq!(p.next_due(), Some(t0 + DEBOUNCE));
    let (full, due) = p.take_due(t0 + DEBOUNCE);
    assert!(!full);
    assert_eq!(due, [(":1.1/StatusNotifierItem".to_string(), false)]);
    assert_eq!(p.items.len(), 1);

    p.note(
      &items,
      &change(":1.7", "/StatusNotifierWatcher", Kind::Watcher),
      t1,
    );
    let (full, due) = p.take_due(t1 + DEBOUNCE);
    assert!(full && due.is_empty() && p.next_due().is_none());
  }
}
