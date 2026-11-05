use anyhow::Result;
use corona_utils::{
  dbus::{owner_changed, sender_signals, subscribe as subscribe_to},
  entity::WriteChangedExt,
  error::ErrorLogExt,
  stream::{next_burst, pump},
};
use futures_lite::Stream;
use gpui_kit::App;
use zbus::Connection;

use crate::{
  Power,
  snapshot::{profiles, snapshot},
};

const UPOWER: &str = "org.freedesktop.UPower";
const POWER_PROFILES: &str = "org.freedesktop.UPower.PowerProfiles";

pub async fn subscribe(conn: &Connection) -> Result<impl Stream<Item = ()> + Unpin + use<>> {
  let rules = [
    sender_signals(UPOWER)?,
    sender_signals(POWER_PROFILES)?,
    owner_changed(UPOWER)?,
    owner_changed(POWER_PROFILES)?,
  ];
  Ok(subscribe_to(conn, rules).await?)
}

pub fn listener(
  cx: &mut App,
  conn: Connection,
  changes: impl Stream<Item = ()> + Unpin + 'static,
  state: Power,
) {
  let mut changes = pump(cx, changes);
  cx.spawn(async move |cx| {
    loop {
      // a failed snapshot clears everything, like profiles, instead of going stale
      let snapshot = snapshot(&conn).await.log_err().ok();
      let (status, battery, devices, backlight) = match snapshot {
        Some(s) => (Some(s.status), s.battery, s.devices, s.keyboard_backlight),
        None => (None, None, Vec::new(), None),
      };
      state.status.write_changed(cx, status);
      state.battery.write_changed(cx, battery);
      state.devices.write_changed(cx, devices);
      state.keyboard_backlight.write_changed(cx, backlight);
      state.profiles.write_changed(cx, profiles(&conn).await);

      if next_burst(&mut changes).await.is_none() {
        break;
      }
    }
  })
  .detach();
}
