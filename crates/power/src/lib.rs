use anyhow::{Context, Result};
use gpui_kit::{App, AppContext, Entity, Global};
use upower_dbus::KbdBacklightProxy;
use zbus::{Connection, proxy::CacheProperties};

use crate::{
  charge::ChargeThresholdProxy,
  listener::{listener, subscribe},
  profiles::PowerProfilesProxy,
};

pub use crate::state::{
  Battery, BatteryLevel, BatteryState, BatteryType, ChargeThreshold, KeyboardBacklight,
  PowerDevice, Profiles, Status,
};

mod charge;
mod listener;
mod profiles;
mod snapshot;
mod state;

#[derive(Clone)]
pub struct Power {
  pub status: Entity<Option<Status>>,
  pub battery: Entity<Option<Battery>>,
  pub devices: Entity<Vec<PowerDevice>>,
  pub profiles: Entity<Option<Profiles>>,
  pub keyboard_backlight: Entity<Option<KeyboardBacklight>>,
  conn: Connection,
}

impl Global for Power {}

pub trait PowerExt {
  fn power(&self) -> &Power;
}

impl PowerExt for App {
  fn power(&self) -> &Power {
    self.global::<Power>()
  }
}

impl Power {
  pub fn status<'c>(&self, cx: &'c App) -> Option<&'c Status> {
    self.status.read(cx).as_ref()
  }

  pub fn battery<'c>(&self, cx: &'c App) -> Option<&'c Battery> {
    self.battery.read(cx).as_ref()
  }

  pub fn list_devices<'c>(&self, cx: &'c App) -> &'c [PowerDevice] {
    self.devices.read(cx)
  }

  pub fn profiles<'c>(&self, cx: &'c App) -> Option<&'c Profiles> {
    self.profiles.read(cx).as_ref()
  }

  pub fn keyboard_backlight<'c>(&self, cx: &'c App) -> Option<&'c KeyboardBacklight> {
    self.keyboard_backlight.read(cx).as_ref()
  }

  /// `power-saver`, `balanced` or `performance`
  pub fn set_profile(&self, profile: String) -> impl Future<Output = Result<()>> + use<> {
    let conn = self.conn.clone();
    async move {
      let proxy = PowerProfilesProxy::builder(&conn)
        .cache_properties(CacheProperties::No)
        .build()
        .await?;
      Ok(proxy.set_active_profile(&profile).await?)
    }
  }

  pub fn set_keyboard_brightness(
    &self,
    brightness: i32,
    cx: &App,
  ) -> impl Future<Output = Result<()>> + use<> {
    let conn = self.conn.clone();
    let max = self.keyboard_backlight(cx).map(|k| k.max);
    async move {
      let max = max.context("no keyboard backlight")?;
      let proxy = KbdBacklightProxy::builder(&conn)
        .cache_properties(CacheProperties::No)
        .build()
        .await?;
      Ok(proxy.set_brightness(brightness.clamp(0, max)).await?)
    }
  }

  pub fn set_charge_threshold(
    &self,
    enabled: bool,
    cx: &App,
  ) -> impl Future<Output = Result<()>> + use<> {
    let conn = self.conn.clone();
    let battery = self
      .battery(cx)
      .and_then(|b| b.charge_threshold.as_ref())
      .map(|t| t.battery.clone());
    async move {
      let battery = battery.context("the battery has no charge limit")?;
      let proxy = ChargeThresholdProxy::builder(&conn)
        .path(battery)?
        .cache_properties(CacheProperties::No)
        .build()
        .await?;
      Ok(proxy.enable_charge_threshold(enabled).await?)
    }
  }
}

pub async fn init(cx: &mut App, conn: &Connection) -> Result<()> {
  let state = Power {
    status: cx.new(|_| None),
    battery: cx.new(|_| None),
    devices: cx.new(|_| Vec::new()),
    profiles: cx.new(|_| None),
    keyboard_backlight: cx.new(|_| None),
    conn: conn.clone(),
  };

  // subscribe before the first snapshot so no change can slip in between
  let changes = subscribe(conn).await?;
  listener(cx, conn.clone(), changes, state.clone());
  cx.set_global(state);

  Ok(())
}
