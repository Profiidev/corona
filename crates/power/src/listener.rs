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
  // a daemon (re)starting sends no signal of its own, its name changing owner says it
  let owner = |name| -> Result<MatchRule<'static>> {
    Ok(
      MatchRule::builder()
        .msg_type(Type::Signal)
        .sender("org.freedesktop.DBus")?
        .interface("org.freedesktop.DBus")?
        .member("NameOwnerChanged")?
        .arg(0, name)?
        .build(),
    )
  };
  let stream = async |rule| MessageStream::for_match_rule(rule, conn, None).await;
  let upower = stream(signals(UPOWER)?).await?;
  let power_profiles = stream(signals(POWER_PROFILES)?).await?;
  let upower_owner = stream(owner(UPOWER)?).await?;
  let power_profiles_owner = stream(owner(POWER_PROFILES)?).await?;
  Ok(
    upower
      .map(|_| ())
      .or(power_profiles.map(|_| ()))
      .or(upower_owner.map(|_| ()))
      .or(power_profiles_owner.map(|_| ())),
  )
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
