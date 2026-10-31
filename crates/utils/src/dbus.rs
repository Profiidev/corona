//! What the D-Bus listeners share: their match rules, their change streams and reading a
//! property out of a snapshot

use std::{collections::HashMap, pin::Pin};

use futures_lite::{Stream, StreamExt};
use zbus::{Connection, MatchRule, MessageStream, message::Type, zvariant::OwnedValue};

/// Every signal `name` sends
pub fn sender_signals(name: &str) -> zbus::Result<MatchRule<'static>> {
  Ok(
    MatchRule::builder()
      .msg_type(Type::Signal)
      .sender(name.to_string())?
      .build(),
  )
}

/// `name` changing owner: a daemon (re)starting sends no signal of its own, this says it
pub fn owner_changed(name: &str) -> zbus::Result<MatchRule<'static>> {
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

/// Fires on every signal matching any of `rules`
pub async fn subscribe(
  conn: &Connection,
  rules: impl IntoIterator<Item = MatchRule<'static>>,
) -> zbus::Result<Pin<Box<dyn Stream<Item = ()> + Send>>> {
  let mut changes: Option<Pin<Box<dyn Stream<Item = ()> + Send>>> = None;
  for rule in rules {
    let signals = MessageStream::for_match_rule(rule, conn, None)
      .await?
      .map(|_| ());
    changes = Some(match changes {
      Some(changes) => Box::pin(changes.or(signals)),
      None => Box::pin(signals),
    });
  }
  Ok(changes.unwrap_or_else(|| Box::pin(futures_lite::stream::empty())))
}

/// Property `key` of an object's `props`, when it is there and a `T`
pub fn get<T: TryFrom<OwnedValue>>(props: &HashMap<String, OwnedValue>, key: &str) -> Option<T> {
  T::try_from(props.get(key)?.try_clone().ok()?).ok()
}
