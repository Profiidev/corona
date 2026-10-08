use anyhow::{Context, Result};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{App, AppContext, Entity, Global, Task};
use upower_dbus::KbdBacklightProxy;
use zbus::{Connection, proxy::CacheProperties};

use crate::{
  charge::ChargeThresholdProxy,
  listener::{listener, subscribe},
  profiles::PowerProfilesProxy,
};

pub use crate::session::{EntryTitle, SessionAction, SessionCapabilities, entry_title};
pub use crate::state::{
  Battery, BatteryLevel, BatteryState, BatteryType, ChargeThreshold, KeyboardBacklight,
  PowerDevice, Profiles, Status,
};

mod charge;
mod listener;
mod profiles;
mod session;
mod snapshot;
mod state;

/// how long before a session watch that failed starts over
const RETRY: std::time::Duration = std::time::Duration::from_secs(5);

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

  pub fn session_capabilities(&self) -> impl Future<Output = Result<SessionCapabilities>> + use<> {
    session::capabilities(self.conn.clone())
  }

  pub fn session_action(&self, action: SessionAction) -> impl Future<Output = Result<()>> + use<> {
    session::run(self.conn.clone(), action)
  }

  pub fn reboot_to(&self, entry: String) -> impl Future<Output = Result<()>> + use<> {
    session::reboot_to(self.conn.clone(), entry)
  }

  /// Watches again after any error, an unlocked sleep is worse than a retry
  pub fn before_sleep(&self, cx: &mut App, before_sleep: impl Fn(&mut App) -> Task<()> + 'static) {
    let conn = self.conn.clone();
    cx.spawn(async move |cx| {
      loop {
        let _ = session::before_sleep(conn.clone(), cx, &before_sleep)
          .await
          .log_err();
        cx.background_executor().timer(RETRY).await;
      }
    })
    .detach();
  }

  /// `loginctl lock-session` and `unlock-session` for this session
  pub fn lock_requests(&self, cx: &mut App, on: impl Fn(bool, &mut App) + 'static) {
    let conn = self.conn.clone();
    cx.spawn(async move |cx| {
      loop {
        let _ = session::lock_requests(conn.clone(), cx, &on)
          .await
          .log_err();
        cx.background_executor().timer(RETRY).await;
      }
    })
    .detach();
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

#[cfg(test)]
mod tests {
  use std::{
    collections::HashMap,
    io::Read,
    os::unix::net::UnixStream,
    sync::{Arc, Mutex},
    time::Duration,
  };

  use corona_utils::test_bus::{TestBus, settle, wait_until};
  use futures_lite::future::block_on;
  use gpui_kit::{self as gpui, TestAppContext};
  use zbus::{
    interface,
    zvariant::{self, ObjectPath, OwnedValue, Value},
  };

  use super::*;

  type Calls = Arc<Mutex<Vec<String>>>;

  fn record(calls: &Calls, call: impl Into<String>) {
    calls.lock().unwrap().push(call.into());
  }

  struct UPower {
    devices: Vec<&'static str>,
    on_battery: bool,
    lid: Option<bool>,
  }

  #[interface(name = "org.freedesktop.UPower")]
  impl UPower {
    fn enumerate_devices(&self) -> Vec<ObjectPath<'static>> {
      self
        .devices
        .iter()
        .map(|d| ObjectPath::from_static_str_unchecked(d))
        .collect()
    }
    fn get_critical_action(&self) -> String {
      "HybridSleep".into()
    }
    fn get_display_device(&self) -> ObjectPath<'static> {
      ObjectPath::from_static_str_unchecked(DISPLAY)
    }
    #[zbus(property)]
    fn on_battery(&self) -> bool {
      self.on_battery
    }
    #[zbus(property)]
    fn lid_is_present(&self) -> bool {
      self.lid.is_some()
    }
    #[zbus(property)]
    fn lid_is_closed(&self) -> bool {
      self.lid.unwrap_or(false)
    }
  }

  #[derive(Clone)]
  struct Device {
    calls: Calls,
    kind: u32,
    power_supply: bool,
    present: bool,
    model: &'static str,
    percentage: f64,
    state: u32,
    time_to_empty: i64,
    threshold: bool,
    fail_threshold: bool,
    fail_percentage: bool,
  }

  impl Device {
    fn new(calls: &Calls, kind: BatteryType, power_supply: bool, model: &'static str) -> Self {
      Self {
        calls: calls.clone(),
        kind: kind as u32,
        power_supply,
        present: true,
        model,
        percentage: 50.,
        state: BatteryState::Discharging as u32,
        time_to_empty: 0,
        threshold: false,
        fail_threshold: false,
        fail_percentage: false,
      }
    }
  }

  #[interface(name = "org.freedesktop.UPower.Device")]
  impl Device {
    fn enable_charge_threshold(&self, enabled: bool) -> zbus::fdo::Result<()> {
      if self.fail_threshold {
        return Err(zbus::fdo::Error::Failed("threshold failed".into()));
      }
      record(
        &self.calls,
        format!("EnableChargeThreshold {} {enabled}", self.model),
      );
      Ok(())
    }
    #[zbus(property, name = "Type")]
    fn kind(&self) -> u32 {
      self.kind
    }
    #[zbus(property)]
    fn power_supply(&self) -> bool {
      self.power_supply
    }
    #[zbus(property)]
    fn is_present(&self) -> bool {
      self.present
    }
    #[zbus(property)]
    fn model(&self) -> String {
      self.model.into()
    }
    #[zbus(property)]
    fn percentage(&self) -> zbus::fdo::Result<f64> {
      if self.fail_percentage {
        return Err(zbus::fdo::Error::Failed("read failed".into()));
      }
      Ok(self.percentage)
    }
    #[zbus(property)]
    fn state(&self) -> u32 {
      self.state
    }
    #[zbus(property)]
    fn battery_level(&self) -> u32 {
      BatteryLevel::Normal as u32
    }
    #[zbus(property)]
    fn time_to_empty(&self) -> i64 {
      self.time_to_empty
    }
    #[zbus(property)]
    fn time_to_full(&self) -> i64 {
      0
    }
    #[zbus(property)]
    fn energy_rate(&self) -> f64 {
      7.5
    }
    #[zbus(property)]
    fn energy(&self) -> f64 {
      30.
    }
    #[zbus(property)]
    fn energy_full(&self) -> f64 {
      60.
    }
    #[zbus(property)]
    fn energy_full_design(&self) -> f64 {
      66.
    }
    #[zbus(property)]
    fn capacity(&self) -> f64 {
      90.9
    }
    #[zbus(property)]
    fn charge_threshold_supported(&self) -> bool {
      self.threshold
    }
    #[zbus(property)]
    fn charge_threshold_enabled(&self) -> bool {
      true
    }
    #[zbus(property)]
    fn charge_start_threshold(&self) -> u32 {
      40
    }
    #[zbus(property)]
    fn charge_end_threshold(&self) -> u32 {
      80
    }
  }

  struct Kbd {
    calls: Calls,
    max: i32,
    fail: bool,
  }

  #[interface(name = "org.freedesktop.UPower.KbdBacklight")]
  impl Kbd {
    fn get_brightness(&self) -> i32 {
      1
    }
    fn get_max_brightness(&self) -> i32 {
      self.max
    }
    fn set_brightness(&self, value: i32) -> zbus::fdo::Result<()> {
      if self.fail {
        return Err(zbus::fdo::Error::Failed("hardware error".into()));
      }
      record(&self.calls, format!("SetBrightness {value}"));
      Ok(())
    }
  }

  struct PowerProfiles {
    active: String,
  }

  #[interface(name = "org.freedesktop.UPower.PowerProfiles")]
  impl PowerProfiles {
    #[zbus(property)]
    fn active_profile(&self) -> String {
      self.active.clone()
    }
    #[zbus(property)]
    fn set_active_profile(&mut self, profile: String) -> zbus::fdo::Result<()> {
      if profile == "invalid-profile" {
        return Err(zbus::fdo::Error::InvalidArgs("unknown profile".into()));
      }
      self.active = profile;
      Ok(())
    }
    #[zbus(property)]
    fn profiles(&self) -> Vec<HashMap<String, OwnedValue>> {
      let profile = |name: &str| {
        HashMap::from([
          (
            "Profile".to_string(),
            Value::from(name).try_to_owned().unwrap(),
          ),
          (
            "Driver".to_string(),
            Value::from("platform").try_to_owned().unwrap(),
          ),
        ])
      };
      // the last one has no name and is skipped
      vec![
        profile("power-saver"),
        profile("balanced"),
        profile("performance"),
        HashMap::new(),
      ]
    }
    #[zbus(property)]
    fn performance_degraded(&self) -> String {
      String::new()
    }
  }

  const BAT0: &str = "/org/freedesktop/UPower/devices/battery_BAT0";
  const DISPLAY: &str = "/org/freedesktop/UPower/devices/DisplayDevice";

  struct Services {
    upower: zbus::Connection,
    calls: Calls,
    profiles: zbus::Connection,
  }

  /// a laptop: a battery with thresholds, line power, a mouse, a gone keyboard
  /// and a second system battery, plus a keyboard backlight and power profiles
  fn services(bus: &TestBus) -> Services {
    services_custom(bus, false, false, false)
  }

  fn services_custom(
    bus: &TestBus,
    fail_kbd: bool,
    fail_threshold: bool,
    fail_display: bool,
  ) -> Services {
    let calls = Calls::default();
    let upower = block_on(async {
      let conn = bus.conn().await;
      let server = conn.object_server();
      let devices = vec![
        "/org/freedesktop/UPower/devices/line_power_AC",
        BAT0,
        "/org/freedesktop/UPower/devices/mouse",
        "/org/freedesktop/UPower/devices/keyboard",
        "/org/freedesktop/UPower/devices/battery_BAT1",
        "/org/freedesktop/UPower/devices/headset",
      ];
      server
        .at(
          "/org/freedesktop/UPower",
          UPower {
            devices,
            on_battery: true,
            lid: Some(false),
          },
        )
        .await
        .unwrap();
      let mut battery = Device::new(&calls, BatteryType::Battery, true, "BAT0");
      battery.threshold = true;
      battery.fail_threshold = fail_threshold;
      let mut display = Device::new(&calls, BatteryType::Battery, true, "display");
      display.percentage = 73.;
      display.time_to_empty = 5400;
      display.fail_percentage = fail_display;
      let mut gone = Device::new(&calls, BatteryType::Keyboard, false, "Keyboard");
      gone.present = false;
      for (path, device) in [
        (
          "/org/freedesktop/UPower/devices/line_power_AC",
          Device::new(&calls, BatteryType::LinePower, true, "AC"),
        ),
        (BAT0, battery),
        (
          "/org/freedesktop/UPower/devices/mouse",
          Device::new(&calls, BatteryType::Mouse, false, "Mouse"),
        ),
        ("/org/freedesktop/UPower/devices/keyboard", gone),
        (
          "/org/freedesktop/UPower/devices/battery_BAT1",
          Device::new(&calls, BatteryType::Battery, true, "BAT1"),
        ),
        (
          "/org/freedesktop/UPower/devices/headset",
          Device::new(&calls, BatteryType::Headset, false, "Headset"),
        ),
        (DISPLAY, display),
      ] {
        server.at(path, device).await.unwrap();
      }
      server
        .at(
          "/org/freedesktop/UPower/KbdBacklight",
          Kbd {
            calls: calls.clone(),
            max: 3,
            fail: fail_kbd,
          },
        )
        .await
        .unwrap();
      conn.request_name("org.freedesktop.UPower").await.unwrap();
      conn
    });
    let profiles = block_on(async {
      let conn = bus.conn().await;
      conn
        .object_server()
        .at(
          "/org/freedesktop/UPower/PowerProfiles",
          PowerProfiles {
            active: "balanced".into(),
          },
        )
        .await
        .unwrap();
      conn
        .request_name("org.freedesktop.UPower.PowerProfiles")
        .await
        .unwrap();
      conn
    });
    Services {
      upower,
      calls,
      profiles,
    }
  }

  fn start(cx: &mut TestAppContext, bus: &TestBus) {
    cx.executor().allow_parking();
    let conn = block_on(bus.conn());
    cx.update(|cx| {
      cx.foreground_executor()
        .clone()
        .block_on(init(cx, &conn))
        .unwrap()
    });
  }

  #[gpui::test]
  fn reads_upower(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let _services = services(&bus);
    start(cx, &bus);
    wait_until(cx, |cx| cx.read(|cx| cx.power().profiles(cx).is_some()));
    cx.read(|cx| {
      let power = cx.power();
      assert_eq!(
        power.status(cx),
        Some(&Status {
          on_battery: true,
          lid_closed: Some(false),
          critical_action: "HybridSleep".into(),
        })
      );
      let battery = power.battery(cx).unwrap();
      // the display device's charge, the system battery's health and thresholds
      assert_eq!(battery.percentage, 73.);
      assert_eq!(battery.time_to_empty, Some(Duration::from_secs(5400)));
      assert_eq!(battery.time_to_full, None);
      assert_eq!((battery.energy_full_design, battery.capacity), (66., 90.9));
      let threshold = battery.charge_threshold.as_ref().unwrap();
      assert_eq!(threshold.battery.as_str(), BAT0);
      assert_eq!(
        (threshold.enabled, threshold.start, threshold.end),
        (true, 40, 80)
      );
      // peripherals only, present ones, by model
      let models: Vec<_> = power
        .list_devices(cx)
        .iter()
        .map(|d| d.model.as_str())
        .collect();
      assert_eq!(models, ["Headset", "Mouse"]);
      assert_eq!(
        power.keyboard_backlight(cx),
        Some(&KeyboardBacklight {
          brightness: 1,
          max: 3
        })
      );
      let profiles = power.profiles(cx).unwrap();
      assert_eq!(profiles.active, "balanced");
      assert_eq!(
        profiles.available,
        ["power-saver", "balanced", "performance"]
      );
      assert_eq!(profiles.degraded, None);
    });
  }

  #[gpui::test]
  fn without_services(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    start(cx, &bus);
    settle(cx);
    cx.read(|cx| {
      let power = cx.power();
      assert!(
        power.status(cx).is_none() && power.battery(cx).is_none() && power.profiles(cx).is_none()
      );
      assert_eq!(
        block_on(power.set_keyboard_brightness(1, cx))
          .unwrap_err()
          .to_string(),
        "no keyboard backlight"
      );
      assert_eq!(
        block_on(power.set_charge_threshold(true, cx))
          .unwrap_err()
          .to_string(),
        "the battery has no charge limit"
      );
      assert!(block_on(power.set_profile("balanced".into())).is_err());
    });
  }

  #[gpui::test]
  fn controls(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let services = services(&bus);
    start(cx, &bus);
    wait_until(cx, |cx| cx.read(|cx| cx.power().profiles(cx).is_some()));
    let power = cx.read(|cx| cx.power().clone());
    block_on(power.set_profile("performance".into())).unwrap();
    wait_until(cx, |cx| {
      cx.read(|cx| cx.power().profiles(cx).unwrap().active == "performance")
    });
    for value in [2, 99, -4] {
      let task = cx.read(|cx| power.set_keyboard_brightness(value, cx));
      block_on(task).unwrap();
    }
    let task = cx.read(|cx| power.set_charge_threshold(false, cx));
    block_on(task).unwrap();
    assert_eq!(
      *services.calls.lock().unwrap(),
      [
        "SetBrightness 2",
        "SetBrightness 3",
        "SetBrightness 0",
        "EnableChargeThreshold BAT0 false"
      ]
    );
  }

  #[gpui::test]
  fn follows_upower_signals(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let services = services(&bus);
    start(cx, &bus);
    wait_until(cx, |cx| cx.read(|cx| cx.power().status(cx).is_some()));
    block_on(async {
      let iface = services
        .upower
        .object_server()
        .interface::<_, UPower>("/org/freedesktop/UPower")
        .await
        .unwrap();
      iface.get_mut().await.on_battery = false;
      iface
        .get()
        .await
        .on_battery_changed(iface.signal_emitter())
        .await
        .unwrap();
    });
    wait_until(cx, |cx| {
      cx.read(|cx| !cx.power().status(cx).unwrap().on_battery)
    });
  }

  #[gpui::test]
  fn upower_restarts_are_noticed(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let services = services(&bus);
    start(cx, &bus);
    wait_until(cx, |cx| cx.read(|cx| cx.power().status(cx).is_some()));
    drop(services);
    let _restarted = block_on(async {
      let conn = bus.conn().await;
      conn
        .object_server()
        .at(
          "/org/freedesktop/UPower",
          UPower {
            devices: vec![],
            on_battery: false,
            lid: None,
          },
        )
        .await
        .unwrap();
      conn.request_name("org.freedesktop.UPower").await.unwrap();
      conn
    });
    wait_until(cx, |cx| {
      cx.read(|cx| !cx.power().status(cx).unwrap().on_battery)
    });
  }

  struct Manager {
    calls: Calls,
    answers: HashMap<&'static str, &'static str>,
    /// our ends of the inhibitor fds handed out
    inhibitors: Arc<Mutex<Vec<UnixStream>>>,
    fail_reboot: bool,
    fail_inhibit: Arc<std::sync::atomic::AtomicBool>,
  }

  impl Manager {
    fn can(&self, what: &str) -> String {
      self.answers.get(what).copied().unwrap_or("yes").into()
    }
  }

  #[interface(name = "org.freedesktop.login1.Manager")]
  impl Manager {
    fn power_off(&self, interactive: bool) {
      record(&self.calls, format!("PowerOff {interactive}"));
    }
    fn reboot(&self, interactive: bool) -> zbus::fdo::Result<()> {
      if self.fail_reboot {
        return Err(zbus::fdo::Error::Failed("reboot rejected".into()));
      }
      record(&self.calls, format!("Reboot {interactive}"));
      Ok(())
    }
    fn suspend(&self, interactive: bool) {
      record(&self.calls, format!("Suspend {interactive}"));
    }
    fn hibernate(&self, interactive: bool) {
      record(&self.calls, format!("Hibernate {interactive}"));
    }
    fn suspend_then_hibernate(&self, interactive: bool) {
      record(&self.calls, format!("SuspendThenHibernate {interactive}"));
    }
    fn can_power_off(&self) -> String {
      self.can("PowerOff")
    }
    fn can_reboot(&self) -> String {
      self.can("Reboot")
    }
    fn can_suspend(&self) -> String {
      self.can("Suspend")
    }
    fn can_hibernate(&self) -> String {
      self.can("Hibernate")
    }
    fn can_suspend_then_hibernate(&self) -> String {
      self.can("SuspendThenHibernate")
    }
    fn can_reboot_to_firmware_setup(&self) -> String {
      self.can("RebootToFirmwareSetup")
    }
    fn can_reboot_to_boot_loader_entry(&self) -> String {
      self.can("RebootToBootLoaderEntry")
    }
    fn set_reboot_to_firmware_setup(&self, enable: bool) {
      record(&self.calls, format!("SetRebootToFirmwareSetup {enable}"));
    }
    fn set_reboot_to_boot_loader_entry(&self, entry: String) {
      record(&self.calls, format!("SetRebootToBootLoaderEntry {entry}"));
    }
    #[zbus(property)]
    fn boot_loader_entries(&self) -> Vec<String> {
      vec!["nixos-generation-1.conf".into(), "auto-windows".into()]
    }
    fn terminate_session(&self, session: String) {
      record(&self.calls, format!("TerminateSession {session}"));
    }
    fn get_session(&self, session: String) -> zbus::fdo::Result<ObjectPath<'static>> {
      match session.as_str() {
        "31" => Ok(ObjectPath::from_static_str_unchecked(
          "/org/freedesktop/login1/session/_331",
        )),
        _ => Err(zbus::fdo::Error::Failed("no such session".into())),
      }
    }
    fn inhibit(&self, what: String, who: String, why: String, mode: String) -> zbus::fdo::Result<zvariant::OwnedFd> {
      if self.fail_inhibit.load(std::sync::atomic::Ordering::Relaxed) {
        return Err(zbus::fdo::Error::Failed("inhibit failed".into()));
      }
      record(&self.calls, format!("Inhibit {what} {who} {why} {mode}"));
      let (ours, theirs) = UnixStream::pair().unwrap();
      ours.set_nonblocking(true).unwrap();
      self.inhibitors.lock().unwrap().push(ours);
      Ok(zvariant::OwnedFd::from(std::os::fd::OwnedFd::from(theirs)))
    }
  }

  struct Logind {
    conn: zbus::Connection,
    calls: Calls,
    inhibitors: Arc<Mutex<Vec<UnixStream>>>,
    fail_inhibit: Arc<std::sync::atomic::AtomicBool>,
  }

  impl Logind {
    fn start(bus: &TestBus, answers: HashMap<&'static str, &'static str>) -> Self {
      Self::start_with_flags(bus, answers, false, false)
    }

    fn start_with_flags(
      bus: &TestBus,
      answers: HashMap<&'static str, &'static str>,
      fail_reboot: bool,
      fail_inhibit: bool,
    ) -> Self {
      let calls = Calls::default();
      let inhibitors = Arc::<Mutex<Vec<UnixStream>>>::default();
      let fail_inhibit = Arc::new(std::sync::atomic::AtomicBool::new(fail_inhibit));
      let fail_inhibit_mgr = fail_inhibit.clone();
      let conn = block_on(async {
        let conn = bus.conn().await;
        conn
          .object_server()
          .at(
            "/org/freedesktop/login1",
            Manager {
              calls: calls.clone(),
              answers,
              inhibitors: inhibitors.clone(),
              fail_reboot,
              fail_inhibit: fail_inhibit_mgr,
            },
          )
          .await
          .unwrap();
        conn.request_name("org.freedesktop.login1").await.unwrap();
        conn
      });
      Self {
        conn,
        calls,
        inhibitors,
        fail_inhibit,
      }
    }

    fn calls(&self) -> Vec<String> {
      self.calls.lock().unwrap().clone()
    }

    /// whether the client still holds each inhibitor, oldest first
    fn held(&self) -> Vec<bool> {
      self
        .inhibitors
        .lock()
        .unwrap()
        .iter_mut()
        .map(|ours| !matches!(ours.read(&mut [0; 1]), Ok(0)))
        .collect()
    }

    fn emit(&self, path: &str, interface: &str, member: &str, body: &impl serde_body::Body) {
      block_on(
        self
          .conn
          .emit_signal(None::<&str>, path, interface, member, body),
      )
      .unwrap();
    }

    fn prepare_for_sleep(&self, start: bool) {
      self.emit(
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
        "PrepareForSleep",
        &(start,),
      );
    }
  }

  mod serde_body {
    pub trait Body: zbus::export::serde::Serialize + zbus::zvariant::DynamicType {}
    impl<T: zbus::export::serde::Serialize + zbus::zvariant::DynamicType> Body for T {}
  }

  fn power_on(cx: &mut TestAppContext, bus: &TestBus) -> Power {
    start(cx, bus);
    cx.read(|cx| cx.power().clone())
  }

  #[gpui::test]
  fn session_capabilities(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let _logind = Logind::start(
      &bus,
      HashMap::from([("Hibernate", "challenge"), ("PowerOff", "no")]),
    );
    let power = power_on(cx, &bus);
    let caps = block_on(power.session_capabilities()).unwrap();
    assert_eq!(
      caps,
      SessionCapabilities {
        suspend: true,
        hibernate: true,
        suspend_then_hibernate: true,
        reboot: true,
        power_off: false,
        reboot_to_firmware: true,
        boot_entries: vec!["nixos-generation-1.conf".into(), "auto-windows".into()],
      }
    );
  }

  #[gpui::test]
  fn suspend_then_hibernate_needs_both(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let _logind = Logind::start(
      &bus,
      HashMap::from([("Hibernate", "na"), ("RebootToBootLoaderEntry", "no")]),
    );
    let power = power_on(cx, &bus);
    let caps = block_on(power.session_capabilities()).unwrap();
    assert!(caps.suspend && !caps.hibernate && !caps.suspend_then_hibernate);
    assert!(caps.boot_entries.is_empty());
    // without logind nothing is allowed
    drop(_logind);
    let caps = block_on(power.session_capabilities()).unwrap();
    assert_eq!(caps, SessionCapabilities::default());
  }

  #[gpui::test]
  fn session_actions(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let logind = Logind::start(&bus, HashMap::new());
    let power = power_on(cx, &bus);
    for action in [
      SessionAction::Suspend,
      SessionAction::Hibernate,
      SessionAction::SuspendThenHibernate,
      SessionAction::Reboot,
      SessionAction::PowerOff,
      SessionAction::RebootToFirmware,
    ] {
      block_on(power.session_action(action)).unwrap();
    }
    block_on(power.reboot_to("auto-windows".into())).unwrap();
    unsafe { std::env::set_var("XDG_SESSION_ID", "31") };
    block_on(power.session_action(SessionAction::Logout)).unwrap();
    assert_eq!(
      logind.calls(),
      [
        "Suspend true",
        "Hibernate true",
        "SuspendThenHibernate true",
        "Reboot true",
        "PowerOff true",
        "SetRebootToFirmwareSetup true",
        "Reboot true",
        "SetRebootToBootLoaderEntry auto-windows",
        "Reboot true",
        "TerminateSession 31",
      ]
    );
    unsafe { std::env::remove_var("XDG_SESSION_ID") };
    assert_eq!(
      block_on(power.session_action(SessionAction::Logout))
        .unwrap_err()
        .to_string(),
      "XDG_SESSION_ID is not set"
    );
  }

  #[gpui::test]
  fn sleep_waits_for_the_lock_screen(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let logind = Logind::start(&bus, HashMap::new());
    let power = power_on(cx, &bus);
    let (done_tx, done_rx) = flume::bounded::<()>(1);
    let locked = Arc::new(Mutex::new(0));
    let count = locked.clone();
    cx.update(|cx| {
      power.before_sleep(cx, move |cx| {
        *count.lock().unwrap() += 1;
        let done = done_rx.clone();
        cx.background_spawn(async move {
          done.recv_async().await.ok();
        })
      })
    });
    wait_until(cx, |_| logind.held() == [true]);
    assert_eq!(
      logind.calls(),
      ["Inhibit sleep corona Lock the screen delay"]
    );

    logind.prepare_for_sleep(true);
    wait_until(cx, |_| *locked.lock().unwrap() == 1);
    // still held until the lock screen is up
    settle(cx);
    assert_eq!(logind.held(), [true]);
    done_tx.send(()).unwrap();
    wait_until(cx, |_| logind.held() == [false]);

    // after resume a new inhibitor holds the next sleep
    logind.prepare_for_sleep(false);
    wait_until(cx, |_| logind.held() == [false, true]);
    assert_eq!(*locked.lock().unwrap(), 1);
  }

  #[gpui::test]
  fn sleep_watch_survives_a_bad_signal(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let logind = Logind::start(&bus, HashMap::new());
    let power = power_on(cx, &bus);
    let locked = Arc::new(Mutex::new(0));
    let count = locked.clone();
    cx.update(|cx| {
      power.before_sleep(cx, move |cx| {
        *count.lock().unwrap() += 1;
        cx.background_spawn(async {})
      })
    });
    wait_until(cx, |_| logind.held() == [true]);
    logind.emit(
      "/org/freedesktop/login1",
      "org.freedesktop.login1.Manager",
      "PrepareForSleep",
      &("yes",),
    );
    logind.prepare_for_sleep(true);
    wait_until(cx, |_| *locked.lock().unwrap() == 1);
  }

  #[gpui::test]
  fn sleep_watch_starts_over_when_logind_comes_late(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let power = power_on(cx, &bus);
    cx.update(|cx| power.before_sleep(cx, |cx| cx.background_spawn(async {})));
    settle(cx);
    let logind = Logind::start(&bus, HashMap::new());
    assert!(logind.held().is_empty());
    cx.executor().advance_clock(RETRY);
    wait_until(cx, |_| logind.held() == [true]);
  }

  #[gpui::test]
  fn lock_requests_follow_the_session(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let logind = Logind::start(&bus, HashMap::new());
    let power = power_on(cx, &bus);
    unsafe { std::env::set_var("XDG_SESSION_ID", "31") };
    let requests = Arc::new(Mutex::new(Vec::new()));
    let seen = requests.clone();
    cx.update(|cx| power.lock_requests(cx, move |lock, _| seen.lock().unwrap().push(lock)));
    settle(cx);
    let session = "/org/freedesktop/login1/session/_331";
    logind.emit(session, "org.freedesktop.login1.Session", "Lock", &());
    wait_until(cx, |_| *requests.lock().unwrap() == [true]);
    logind.emit(session, "org.freedesktop.login1.Session", "Unlock", &());
    wait_until(cx, |_| *requests.lock().unwrap() == [true, false]);
    // another session's requests are not ours
    logind.emit(
      "/org/freedesktop/login1/session/_32",
      "org.freedesktop.login1.Session",
      "Lock",
      &(),
    );
    settle(cx);
    assert_eq!(*requests.lock().unwrap(), [true, false]);
  }

  #[gpui::test]
  fn sleep_watch_recovers_after_mid_session_failure(cx: &mut TestAppContext) {
    use std::sync::atomic::Ordering;
    let bus = TestBus::new();
    let logind = Logind::start(&bus, HashMap::new());
    let power = power_on(cx, &bus);
    cx.update(|cx| power.before_sleep(cx, |cx| cx.background_spawn(async {})));
    wait_until(cx, |_| logind.held() == [true]);
    logind.fail_inhibit.store(true, Ordering::Relaxed);
    logind.prepare_for_sleep(true);
    logind.prepare_for_sleep(false);
    settle(cx);
    logind.fail_inhibit.store(false, Ordering::Relaxed);
    cx.executor().advance_clock(RETRY);
    wait_until(cx, |_| logind.held().len() >= 2 && *logind.held().last().unwrap());
  }

  #[gpui::test]
  fn lock_requests_retries_after_missing_session_id(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let logind = Logind::start(&bus, HashMap::new());
    let power = power_on(cx, &bus);
    let requests = Arc::new(Mutex::new(Vec::new()));
    let seen = requests.clone();
    unsafe { std::env::remove_var("XDG_SESSION_ID") };
    cx.update(|cx| power.lock_requests(cx, move |lock, _| seen.lock().unwrap().push(lock)));
    settle(cx);
    unsafe { std::env::set_var("XDG_SESSION_ID", "31") };
    cx.executor().advance_clock(RETRY);
    settle(cx);
    let session = "/org/freedesktop/login1/session/_331";
    logind.emit(session, "org.freedesktop.login1.Session", "Lock", &());
    wait_until(cx, |_| *requests.lock().unwrap() == [true]);
    unsafe { std::env::remove_var("XDG_SESSION_ID") };
  }

  #[gpui::test]
  fn set_keyboard_brightness_dbus_failure(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let _services = services_custom(&bus, true, false, false);
    let power = power_on(cx, &bus);
    wait_until(cx, |cx| cx.read(|cx| cx.power().keyboard_backlight(cx).is_some()));
    let task = cx.read(|cx| power.set_keyboard_brightness(1, cx));
    assert!(block_on(task).is_err());
  }

  #[gpui::test]
  fn set_charge_threshold_failure(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let _services = services_custom(&bus, false, true, false);
    let power = power_on(cx, &bus);
    wait_until(cx, |cx| cx.read(|cx| cx.power().battery(cx).is_some()));
    let task = cx.read(|cx| power.set_charge_threshold(false, cx));
    assert!(block_on(task).is_err());
  }

  #[gpui::test]
  fn set_profile_invalid_profile_rejected(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let _services = services(&bus);
    let power = power_on(cx, &bus);
    wait_until(cx, |cx| cx.read(|cx| cx.power().profiles(cx).is_some()));
    assert!(block_on(power.set_profile("invalid-profile".into())).is_err());
  }

  #[gpui::test]
  fn power_profiles_restart_is_noticed(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let services = services(&bus);
    start(cx, &bus);
    wait_until(cx, |cx| cx.read(|cx| cx.power().profiles(cx).is_some()));
    drop(services.profiles);
    let _restarted = block_on(async {
      let conn = bus.conn().await;
      conn
        .object_server()
        .at(
          "/org/freedesktop/UPower/PowerProfiles",
          PowerProfiles {
            active: "power-saver".into(),
          },
        )
        .await
        .unwrap();
      conn
        .request_name("org.freedesktop.UPower.PowerProfiles")
        .await
        .unwrap();
      conn
    });
    wait_until(cx, |cx| {
      cx.read(|cx| cx.power().profiles(cx).unwrap().active == "power-saver")
    });
  }

  #[gpui::test]
  fn upower_crash_retains_stale_power_state(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let services = services(&bus);
    start(cx, &bus);
    wait_until(cx, |cx| cx.read(|cx| cx.power().status(cx).is_some()));
    let status_before = cx.read(|cx| cx.power().status(cx).cloned()).unwrap();
    let battery_before = cx.read(|cx| cx.power().battery(cx).cloned()).unwrap();
    drop(services.upower);
    settle(cx);
    // State is retained rather than cleared to None
    cx.read(|cx| {
      assert_eq!(cx.power().status(cx), Some(&status_before));
      assert_eq!(cx.power().battery(cx), Some(&battery_before));
    });
  }

  #[gpui::test]
  fn reboot_to_firmware_incomplete_transaction_on_reboot_failure(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let logind = Logind::start_with_flags(&bus, HashMap::new(), true, false);
    let power = power_on(cx, &bus);
    let err = block_on(power.session_action(SessionAction::RebootToFirmware)).unwrap_err();
    assert!(err.to_string().contains("reboot rejected"));
    assert_eq!(logind.calls(), ["SetRebootToFirmwareSetup true"]);
  }

  #[gpui::test]
  fn reboot_to_entry_incomplete_transaction_on_reboot_failure(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let logind = Logind::start_with_flags(&bus, HashMap::new(), true, false);
    let power = power_on(cx, &bus);
    let err = block_on(power.reboot_to("auto-windows".into())).unwrap_err();
    assert!(err.to_string().contains("reboot rejected"));
    assert_eq!(logind.calls(), ["SetRebootToBootLoaderEntry auto-windows"]);
  }

  #[gpui::test]
  fn before_sleep_inhibit_failure(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let _logind = Logind::start_with_flags(&bus, HashMap::new(), false, true);
    let conn = block_on(bus.conn());
    cx.update(|cx| {
      let mut async_app = cx.to_async();
      let res = block_on(session::before_sleep(conn, &mut async_app, |_| Task::ready(())));
      assert!(res.is_err());
    });
  }

  #[gpui::test]
  fn secondary_system_battery_is_omitted(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let _services = services(&bus);
    start(cx, &bus);
    wait_until(cx, |cx| cx.read(|cx| cx.power().battery(cx).is_some()));
    cx.read(|cx| {
      let power = cx.power();
      assert!(power.list_devices(cx).iter().all(|d| d.model != "BAT1"));
    });
  }

  #[gpui::test]
  fn display_failure_drops_battery_status(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let _services = services_custom(&bus, false, false, true);
    start(cx, &bus);
    wait_until(cx, |cx| cx.read(|cx| cx.power().status(cx).is_some()));
    cx.read(|cx| {
      let power = cx.power();
      assert!(power.status(cx).is_some());
      assert!(power.battery(cx).is_none());
    });
  }
}
