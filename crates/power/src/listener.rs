use anyhow::Result;
use corona_utils::{entity::WriteChangedExt, error::ErrorLogExt};
use futures_lite::{Stream, StreamExt, future::poll_once};
use gpui_kit::App;
use zbus::{Connection, MatchRule, MessageStream, message::Type};

use crate::{
  Power,
  snapshot::{profiles, snapshot},
};

const UPOWER: &str = "org.freedesktop.UPower";
const POWER_PROFILES: &str = "org.freedesktop.UPower.PowerProfiles";

pub async fn subscribe(conn: &Connection) -> Result<impl Stream<Item = ()> + Unpin + use<>> {
  let signals = |sender| -> Result<MatchRule<'static>> {
    Ok(
      MatchRule::builder()
        .msg_type(Type::Signal)
        .sender(sender)?
        .build(),
    )
  };
  let upower = MessageStream::for_match_rule(signals(UPOWER)?, conn, None).await?;
  let power_profiles = MessageStream::for_match_rule(signals(POWER_PROFILES)?, conn, None).await?;
  Ok(upower.map(|_| ()).or(power_profiles.map(|_| ())))
}

pub fn listener(
  cx: &mut App,
  conn: Connection,
  mut changes: impl Stream<Item = ()> + Unpin + 'static,
  state: Power,
) {
  cx.spawn(async move |cx| {
    loop {
      if let Ok(snapshot) = snapshot(&conn).await.log_err() {
        state.status.write_changed(cx, Some(snapshot.status));
        state.battery.write_changed(cx, snapshot.battery);
        state.devices.write_changed(cx, snapshot.devices);
        state
          .keyboard_backlight
          .write_changed(cx, snapshot.keyboard_backlight);
      }
      state.profiles.write_changed(cx, profiles(&conn).await);

      if changes.next().await.is_none() {
        break;
      }
      // drain the rest of the burst already queued, one snapshot covers it
      while let Some(Some(())) = poll_once(changes.next()).await {}
    }
  })
  .detach();
}
