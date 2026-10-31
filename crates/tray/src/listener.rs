use anyhow::Result;
use corona_utils::{
  dbus::subscribe as subscribe_to,
  entity::WriteChangedExt,
  error::ErrorLogExt,
  stream::{next_burst, pump},
};
use futures_lite::Stream;
use gpui_kit::App;
use zbus::{Connection, MatchRule, message::Type};

use crate::{
  Tray,
  proxy::{ITEM_INTERFACE, MENU_INTERFACE, WATCHER_NAME},
  snapshot::{Activatable, snapshot},
};

pub async fn subscribe(conn: &Connection) -> Result<impl Stream<Item = ()> + Unpin + use<>> {
  let signal = || MatchRule::builder().msg_type(Type::Signal);
  let rules = [
    signal().interface(WATCHER_NAME)?.build(),
    signal().interface(ITEM_INTERFACE)?.build(),
    signal().interface(MENU_INTERFACE)?.build(),
    signal()
      .interface("org.freedesktop.DBus.Properties")?
      .member("PropertiesChanged")?
      .arg(0, ITEM_INTERFACE)?
      .build(),
  ];
  Ok(subscribe_to(conn, rules).await?)
}

pub fn listener(
  cx: &mut App,
  conn: Connection,
  changes: impl Stream<Item = ()> + Unpin + 'static,
  state: Tray,
) {
  let mut changes = pump(cx, changes);
  cx.spawn(async move |cx| {
    let mut activatable = Activatable::new();
    loop {
      if let Ok(items) = snapshot(&conn, &mut activatable).await.log_err() {
        activatable.retain(|address, _| items.iter().any(|i| &i.address == address));
        state.items.write_changed(cx, items);
      }

      if next_burst(&mut changes).await.is_none() {
        break;
      }
    }
  })
  .detach();
}
