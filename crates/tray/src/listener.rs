use anyhow::Result;
use corona_utils::{entity::WriteChangedExt, error::ErrorLogExt};
use futures_lite::{Stream, StreamExt, future::poll_once};
use gpui_kit::App;
use zbus::{Connection, MatchRule, MessageStream, message::Type};

use crate::{
  Tray,
  proxy::{ITEM_INTERFACE, MENU_INTERFACE, WATCHER_NAME},
  snapshot::snapshot,
};

async fn signals(conn: &Connection, rule: MatchRule<'_>) -> Result<MessageStream> {
  Ok(MessageStream::for_match_rule(rule, conn, None).await?)
}

pub async fn subscribe(conn: &Connection) -> Result<impl Stream<Item = ()> + Unpin + use<>> {
  let signal = || MatchRule::builder().msg_type(Type::Signal);
  let watcher = signals(conn, signal().interface(WATCHER_NAME)?.build()).await?;
  let items = signals(conn, signal().interface(ITEM_INTERFACE)?.build()).await?;
  let menus = signals(conn, signal().interface(MENU_INTERFACE)?.build()).await?;
  let properties = signals(
    conn,
    signal()
      .interface("org.freedesktop.DBus.Properties")?
      .member("PropertiesChanged")?
      .arg(0, ITEM_INTERFACE)?
      .build(),
  )
  .await?;
  Ok(
    watcher
      .map(|_| ())
      .or(items.map(|_| ()))
      .or(menus.map(|_| ()))
      .or(properties.map(|_| ())),
  )
}

pub fn listener(
  cx: &mut App,
  conn: Connection,
  mut changes: impl Stream<Item = ()> + Unpin + 'static,
  state: Tray,
) {
  cx.spawn(async move |cx| {
    loop {
      if let Ok(items) = snapshot(&conn).await.log_err() {
        state.items.write_changed(cx, items);
      }

      if changes.next().await.is_none() {
        break;
      }
      // drain the rest of the burst already queued, one snapshot covers it
      while let Some(Some(())) = poll_once(changes.next()).await {}
    }
  })
  .detach();
}
