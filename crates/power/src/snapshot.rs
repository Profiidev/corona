use anyhow::Result;
use corona_utils::error::ErrorLogExt;
use upower_dbus::{BatteryType, DeviceProxy, KbdBacklightProxy, UPowerProxy};
use zbus::{Connection, proxy::CacheProperties, zvariant::OwnedObjectPath};

use crate::{
  profiles::PowerProfilesProxy,
  state::{
    Battery, ChargeThreshold, KeyboardBacklight, PowerDevice, Profiles, Status, duration,
    is_peripheral,
  },
};

pub struct Snapshot {
  pub status: Status,
  pub battery: Option<Battery>,
  pub devices: Vec<PowerDevice>,
  pub keyboard_backlight: Option<KeyboardBacklight>,
}

async fn device(conn: &Connection, path: OwnedObjectPath) -> Result<DeviceProxy<'static>> {
  Ok(
    DeviceProxy::builder(conn)
      .path(path)?
      .cache_properties(CacheProperties::No)
      .build()
      .await?,
  )
}

pub async fn snapshot(conn: &Connection) -> Result<Snapshot> {
  let upower = UPowerProxy::builder(conn)
    .cache_properties(CacheProperties::No)
    .build()
    .await?;
  let lid_present = upower.lid_is_present().await.unwrap_or(false);
  let status = Status {
    on_battery: upower.on_battery().await.unwrap_or(false),
    lid_closed: if lid_present {
      upower.lid_is_closed().await.ok()
    } else {
      None
    },
    critical_action: upower.get_critical_action().await.unwrap_or_default(),
  };

  // the first battery powering the system holds the charge thresholds
  let mut system_battery = None;
  let mut devices = Vec::new();
  for path in upower.enumerate_devices().await? {
    let Ok(proxy) = device(conn, path.clone()).await.log_err() else {
      continue;
    };
    let kind = proxy.type_().await.unwrap_or(BatteryType::Unknown);
    let power_supply = proxy.power_supply().await.unwrap_or(false);
    if kind == BatteryType::Battery && power_supply {
      system_battery.get_or_insert(path);
    } else if is_peripheral(kind, power_supply) && proxy.is_present().await.unwrap_or(true) {
      devices.push(PowerDevice {
        model: proxy.model().await.unwrap_or_default(),
        kind,
        percentage: proxy.percentage().await.unwrap_or_default(),
        state: proxy
          .state()
          .await
          .unwrap_or(upower_dbus::BatteryState::Unknown),
        level: proxy
          .battery_level()
          .await
          .unwrap_or(upower_dbus::BatteryLevel::Unknown),
        path,
      });
    }
  }
  devices.sort_by(|a, b| a.model.cmp(&b.model));

  let battery = match system_battery {
    Some(path) => battery(conn, &upower, path).await.log_err().ok(),
    None => None,
  };

  Ok(Snapshot {
    status,
    battery,
    devices,
    keyboard_backlight: keyboard_backlight(conn).await,
  })
}

async fn battery(
  conn: &Connection,
  upower: &UPowerProxy<'_>,
  path: OwnedObjectPath,
) -> Result<Battery> {
  // the display device combines every battery, the system one keeps its thresholds
  let display = upower.get_display_device().await?;
  let system = device(conn, path.clone()).await?;
  let charge_threshold = if system.charge_threshold_supported().await.unwrap_or(false) {
    Some(ChargeThreshold {
      battery: path,
      enabled: system.charge_threshold_enabled().await.unwrap_or(false),
      start: system.charge_start_threshold().await.unwrap_or_default(),
      end: system.charge_end_threshold().await.unwrap_or(100),
    })
  } else {
    None
  };
  Ok(Battery {
    percentage: display.percentage().await?,
    state: display.state().await?,
    time_to_empty: duration(display.time_to_empty().await.unwrap_or_default()),
    time_to_full: duration(display.time_to_full().await.unwrap_or_default()),
    energy_rate: display.energy_rate().await.unwrap_or_default(),
    energy: display.energy().await.unwrap_or_default(),
    energy_full: display.energy_full().await.unwrap_or_default(),
    energy_full_design: system.energy_full_design().await.unwrap_or_default(),
    capacity: system.capacity().await.unwrap_or_default(),
    charge_threshold,
  })
}

async fn keyboard_backlight(conn: &Connection) -> Option<KeyboardBacklight> {
  let proxy = KbdBacklightProxy::builder(conn)
    .cache_properties(CacheProperties::No)
    .build()
    .await
    .ok()?;
  let max = proxy
    .get_max_brightness()
    .await
    .ok()
    .filter(|max| *max > 0)?;
  Some(KeyboardBacklight {
    brightness: proxy.get_brightness().await.ok()?,
    max,
  })
}

pub async fn profiles(conn: &Connection) -> Option<Profiles> {
  let proxy = PowerProfilesProxy::builder(conn)
    .cache_properties(CacheProperties::No)
    .build()
    .await
    .ok()?;
  let available = proxy
    .profiles()
    .await
    .ok()?
    .into_iter()
    .filter_map(|profile| String::try_from(profile.get("Profile")?.try_clone().ok()?).ok())
    .collect();
  Some(Profiles {
    active: proxy.active_profile().await.ok()?,
    available,
    degraded: proxy
      .performance_degraded()
      .await
      .ok()
      .filter(|reason| !reason.is_empty()),
  })
}

#[cfg(test)]
mod tests {
  /// reads this machine's power state: `cargo test -p corona_power -- --ignored --nocapture`
  #[test]
  #[ignore]
  fn live_snapshot() {
    zbus::block_on(async {
      let conn = zbus::Connection::system().await.unwrap();
      let snapshot = super::snapshot(&conn).await.unwrap();
      println!("{:#?}", snapshot.status);
      println!("{:#?}", snapshot.battery);
      println!("{:#?}", snapshot.devices);
      println!("{:#?}", snapshot.keyboard_backlight);
      println!("{:#?}", super::profiles(&conn).await);
    });
  }
}
