use anyhow::Result;
use corona_utils::error::ErrorLogExt;
use futures_lite::{Stream, StreamExt, future::poll_once};
use gpui_kit::{App, AsyncApp, Entity};
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
        write_changed(cx, &state.status, Some(snapshot.status));
        write_changed(cx, &state.battery, snapshot.battery);
        write_changed(cx, &state.devices, snapshot.devices);
        write_changed(cx, &state.keyboard_backlight, snapshot.keyboard_backlight);
      }
      write_changed(cx, &state.profiles, profiles(&conn).await);

      if changes.next().await.is_none() {
        break;
      }
      // drain the rest of the burst already queued, one snapshot covers it
      while let Some(Some(())) = poll_once(changes.next()).await {}
    }
  })
  .detach();
}

fn write_changed<T: PartialEq + 'static>(cx: &mut AsyncApp, entity: &Entity<T>, next: T) {
  cx.update(|cx| {
    if *entity.read(cx) != next {
      entity.write(cx, next);
    }
  });
}
