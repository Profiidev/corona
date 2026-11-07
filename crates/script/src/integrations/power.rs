use std::time::UNIX_EPOCH;

use corona_power as pw;
use corona_power::PowerExt;
use corona_js::{
  Cx, Glob, Json, Listener, ModuleBuilder, PluginRef, ScriptModule, Throws, Unsubscribe,
  host_module, json,
};
use gpui_kit::App;
use serde::{Deserialize, Serialize};
use specta::{Type, specta};

#[derive(Serialize, Type)]
#[serde(rename_all = "snake_case")]
enum BatteryState {
  Unknown,
  Charging,
  Discharging,
  Empty,
  FullyCharged,
  PendingCharge,
  PendingDischarge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
enum SessionAction {
  Logout,
  Suspend,
  Hibernate,
  SuspendThenHibernate,
  Reboot,
  PowerOff,
  /// Reboot into the firmware setup.
  RebootToFirmware,
}

impl From<SessionAction> for pw::SessionAction {
  fn from(action: SessionAction) -> Self {
    match action {
      SessionAction::Logout => Self::Logout,
      SessionAction::Suspend => Self::Suspend,
      SessionAction::Hibernate => Self::Hibernate,
      SessionAction::SuspendThenHibernate => Self::SuspendThenHibernate,
      SessionAction::Reboot => Self::Reboot,
      SessionAction::PowerOff => Self::PowerOff,
      SessionAction::RebootToFirmware => Self::RebootToFirmware,
    }
  }
}

/// What this machine allows.
#[derive(Serialize, Type)]
struct SessionCapabilities {
  suspend: bool,
  hibernate: bool,
  suspend_then_hibernate: bool,
  reboot: bool,
  power_off: bool,
  reboot_to_firmware: bool,
  /// Boot loader entries `rebootTo` takes, empty when the boot loader can't be told.
  boot_entries: Vec<String>,
}

impl From<pw::SessionCapabilities> for SessionCapabilities {
  fn from(c: pw::SessionCapabilities) -> Self {
    Self {
      suspend: c.suspend,
      hibernate: c.hibernate,
      suspend_then_hibernate: c.suspend_then_hibernate,
      reboot: c.reboot,
      power_off: c.power_off,
      reboot_to_firmware: c.reboot_to_firmware,
      boot_entries: c.boot_entries,
    }
  }
}

#[derive(Serialize, Type)]
struct Status {
  on_battery: bool,
  lid_closed: Option<bool>,
  critical_action: String,
}

#[derive(Serialize, Type)]
struct ChargeThreshold {
  enabled: bool,
  start: u32,
  end: u32,
}

#[derive(Serialize, Type)]
struct Battery {
  percentage: f64,
  state: BatteryState,
  time_to_empty: Option<f64>,
  time_to_full: Option<f64>,
  energy_rate: f64,
  energy: f64,
  energy_full: f64,
  energy_full_design: f64,
  capacity: f64,
  charge_threshold: Option<ChargeThreshold>,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "snake_case")]
enum DeviceKind {
  Mouse,
  Keyboard,
  Headset,
  Headphones,
  Speakers,
  GamingInput,
  Phone,
  Tablet,
  Pen,
  Touchpad,
  Wearable,
  RemoteControl,
  Other,
}

#[derive(Serialize, Type)]
struct Device {
  model: String,
  kind: DeviceKind,
  percentage: f64,
  state: BatteryState,
}

#[derive(Serialize, Type)]
struct Profiles {
  active: String,
  available: Vec<String>,
  degraded: Option<String>,
}

#[derive(Serialize, Type)]
struct SleepEvent {
  /// `true` right before suspend or hibernate, `false` after resume.
  sleeping: bool,
}

#[derive(Serialize, Type)]
struct KeyboardBacklight {
  brightness: i32,
  max: i32,
}

impl From<pw::BatteryState> for BatteryState {
  fn from(state: pw::BatteryState) -> Self {
    match state {
      pw::BatteryState::Unknown => BatteryState::Unknown,
      pw::BatteryState::Charging => BatteryState::Charging,
      pw::BatteryState::Discharging => BatteryState::Discharging,
      pw::BatteryState::Empty => BatteryState::Empty,
      pw::BatteryState::FullyCharged => BatteryState::FullyCharged,
      pw::BatteryState::PendingCharge => BatteryState::PendingCharge,
      pw::BatteryState::PendingDischarge => BatteryState::PendingDischarge,
    }
  }
}

impl From<pw::BatteryType> for DeviceKind {
  fn from(kind: pw::BatteryType) -> Self {
    match kind {
      pw::BatteryType::Mouse => DeviceKind::Mouse,
      pw::BatteryType::Keyboard => DeviceKind::Keyboard,
      pw::BatteryType::Headset => DeviceKind::Headset,
      pw::BatteryType::Headphones => DeviceKind::Headphones,
      pw::BatteryType::Speakers => DeviceKind::Speakers,
      pw::BatteryType::GamingInput => DeviceKind::GamingInput,
      pw::BatteryType::Phone => DeviceKind::Phone,
      pw::BatteryType::Tablet => DeviceKind::Tablet,
      pw::BatteryType::Pen => DeviceKind::Pen,
      pw::BatteryType::Touchpad => DeviceKind::Touchpad,
      pw::BatteryType::Wearable => DeviceKind::Wearable,
      pw::BatteryType::RemoteControl => DeviceKind::RemoteControl,
      _ => DeviceKind::Other,
    }
  }
}

impl From<&pw::Battery> for Battery {
  fn from(battery: &pw::Battery) -> Self {
    Self {
      percentage: battery.percentage,
      state: battery.state.into(),
      time_to_empty: battery.time_to_empty.map(|d| d.as_secs_f64()),
      time_to_full: battery.time_to_full.map(|d| d.as_secs_f64()),
      energy_rate: battery.energy_rate,
      energy: battery.energy,
      energy_full: battery.energy_full,
      energy_full_design: battery.energy_full_design,
      capacity: battery.capacity,
      charge_threshold: battery.charge_threshold.as_ref().map(|t| ChargeThreshold {
        enabled: t.enabled,
        start: t.start,
        end: t.end,
      }),
    }
  }
}

type Pw = Glob<pw::Power>;

/// Calls `cb` with each suspend and resume, as logind announces them. For services that
/// reconnect after a resume.
#[specta]
fn on_sleep(mut cx: Cx, cb: Listener<Json<SleepEvent>>) -> Unsubscribe {
  let (listener, sleep) = (cb.clone(), cx.power().sleep.clone());
  let sub = cx.observe(&sleep, move |sleep, cx| {
    let sleeping = sleep.read(cx).sleeping;
    listener.call(cx, Json(SleepEvent { sleeping }));
  });
  cb.keep(sub)
}

/// `power-saver`, `balanced` or `performance`.
#[specta]
async fn set_profile(power: Pw, profile: String) -> Throws<()> {
  power.set_profile(profile).await.into()
}

#[specta]
async fn set_keyboard_brightness(cx: Cx, power: Pw, brightness: i32) -> Throws<()> {
  let set = power.set_keyboard_brightness(brightness, &cx);
  set.await.into()
}

#[specta]
async fn session_capabilities(power: Pw) -> Throws<Json<SessionCapabilities>> {
  let capabilities = power.session_capabilities().await;
  capabilities.map(|c| Json(c.into())).into()
}

#[specta]
async fn session_action(power: Pw, action: Json<SessionAction>) -> Throws<()> {
  power.session_action(action.0.into()).await.into()
}

/// Reboots into one of `sessionCapabilities().boot_entries`.
#[specta]
async fn reboot_to(power: Pw, entry: String) -> Throws<()> {
  power.reboot_to(entry).await.into()
}

#[specta]
async fn set_charge_threshold(cx: Cx, power: Pw, enabled: bool) -> Throws<()> {
  let set = power.set_charge_threshold(enabled, &cx);
  set.await.into()
}

/// `corona/power`
pub struct PowerModule;

impl ScriptModule for PowerModule {
  const NAME: &str = "power";
  const DOC: &str = "Battery, power profiles and the session (suspend, reboot, …).";

  fn module(_: &PluginRef, m: &mut ModuleBuilder, _: &mut App) {
    m.watch(
      "status",
      "",
      |cx| cx.power().status.clone(),
      json(|cx| {
        cx.power().status(cx).map(|s| Status {
          on_battery: s.on_battery,
          lid_closed: s.lid_closed,
          critical_action: s.critical_action.clone(),
        })
      }),
    );
    m.watch(
      "battery",
      "",
      |cx| cx.power().battery.clone(),
      json(|cx| cx.power().battery(cx).map(Battery::from)),
    );
    m.watch(
      "listDevices",
      "",
      |cx| cx.power().devices.clone(),
      json(|cx| {
        let devices = cx.power().list_devices(cx);
        devices
          .iter()
          .map(|d| Device {
            model: d.model.clone(),
            kind: d.kind.into(),
            percentage: d.percentage,
            state: d.state.into(),
          })
          .collect::<Vec<_>>()
      }),
    );
    m.watch(
      "profiles",
      "",
      |cx| cx.power().profiles.clone(),
      json(|cx| {
        cx.power().profiles(cx).map(|p| Profiles {
          active: p.active.clone(),
          available: p.available.clone(),
          degraded: p.degraded.clone(),
        })
      }),
    );
    m.watch(
      "keyboardBacklight",
      "",
      |cx| cx.power().keyboard_backlight.clone(),
      json(|cx| {
        cx.power()
          .keyboard_backlight(cx)
          .map(|k| KeyboardBacklight {
            brightness: k.brightness,
            max: k.max,
          })
      }),
    );
    m.watch(
      "sleeping",
      "",
      |cx| cx.power().sleep.clone(),
      json(|cx| cx.power().sleep(cx).sleeping),
    );
    m.watch(
      "resumedAt",
      "Unix seconds of the last resume; null before the first one or while asleep.",
      |cx| cx.power().sleep.clone(),
      json(|cx| {
        let sleep = cx.power().sleep(cx);
        (sleep.changed_at)
          .filter(|_| !sleep.sleeping)
          .map(|t| t.duration_since(UNIX_EPOCH).map_or(0., |d| d.as_secs_f64()))
      }),
    );
    host_module!(m, {
      sync: [on_sleep],
      async: [
        set_profile,
        set_keyboard_brightness,
        session_capabilities,
        session_action,
        reboot_to,
        set_charge_threshold,
      ],
    });
  }
}

#[cfg(test)]
mod tests {
  use std::time::Duration;

  use corona_power as pw;
  use corona_utils::test_bus::{TestBus, wait_until};
  use futures_lite::future::block_on;
  use gpui_kit::{self as gpui, TestAppContext};
  use serde_json::json;

  use super::{Battery, BatteryState, DeviceKind, SessionAction, SessionCapabilities};
  use crate::harness;

  #[gpui::test]
  fn sleep_reaches_the_plugin(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (bus, conn) = TestBus::with_conn();
    cx.update(|cx| {
      let executor = cx.foreground_executor().clone();
      executor.block_on(pw::connect(cx, &conn)).unwrap()
    });
    let logind = block_on(async {
      let logind = bus.conn().await;
      logind.request_name("org.freedesktop.login1").await.unwrap();
      logind
    });
    let sleep = |start: bool| {
      block_on(logind.emit_signal(
        None::<&str>,
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
        "PrepareForSleep",
        &(start,),
      ))
      .unwrap()
    };

    let body = r#"if (!globalThis.started) {
      globalThis.started = true;
      m.onSleep((e) => report({ event: e }));
    }
    report([m.sleeping(), m.resumedAt()]);"#;
    let (view, cx) = harness::module::<super::PowerModule>(cx, body);
    assert_eq!(view.last(cx), json!([false, null]));

    // the watch may not be up yet, so asleep until it is
    wait_until(cx, |cx| {
      sleep(true);
      view.last(cx) == json!([true, null])
    });
    assert!(
      view
        .reports(cx)
        .contains(&json!({ "event": { "sleeping": true } }))
    );
    wait_until(cx, |cx| {
      sleep(false);
      view.last(cx)[0] == false
    });
    assert!(view.last(cx)[1].as_f64().unwrap() > 1e9);
  }

  fn json(value: impl serde::Serialize) -> serde_json::Value {
    serde_json::to_value(value).unwrap()
  }

  #[test]
  fn battery_states() {
    let all = [
      (pw::BatteryState::Unknown, "unknown"),
      (pw::BatteryState::Charging, "charging"),
      (pw::BatteryState::Discharging, "discharging"),
      (pw::BatteryState::Empty, "empty"),
      (pw::BatteryState::FullyCharged, "fully_charged"),
      (pw::BatteryState::PendingCharge, "pending_charge"),
      (pw::BatteryState::PendingDischarge, "pending_discharge"),
    ];
    for (state, name) in all {
      assert_eq!(json(BatteryState::from(state)), name);
    }
  }

  #[test]
  fn device_kinds() {
    let all = [
      (pw::BatteryType::Mouse, "mouse"),
      (pw::BatteryType::Keyboard, "keyboard"),
      (pw::BatteryType::Headset, "headset"),
      (pw::BatteryType::Headphones, "headphones"),
      (pw::BatteryType::Speakers, "speakers"),
      (pw::BatteryType::GamingInput, "gaming_input"),
      (pw::BatteryType::Phone, "phone"),
      (pw::BatteryType::Tablet, "tablet"),
      (pw::BatteryType::Pen, "pen"),
      (pw::BatteryType::Touchpad, "touchpad"),
      (pw::BatteryType::Wearable, "wearable"),
      (pw::BatteryType::RemoteControl, "remote_control"),
      // everything else
      (pw::BatteryType::Unknown, "other"),
      (pw::BatteryType::Battery, "other"),
      (pw::BatteryType::Printer, "other"),
    ];
    for (kind, name) in all {
      assert_eq!(json(DeviceKind::from(kind)), name, "{kind:?}");
    }
  }

  #[test]
  fn battery() {
    let battery = pw::Battery {
      percentage: 55.5,
      state: pw::BatteryState::Discharging,
      time_to_empty: Some(Duration::from_millis(90_500)),
      time_to_full: None,
      energy_rate: 7.5,
      energy: 30.,
      energy_full: 50.,
      energy_full_design: 60.,
      capacity: 83.3,
      charge_threshold: None,
    };
    let json = json(Battery::from(&battery));
    assert_eq!(json["percentage"], 55.5);
    assert_eq!(json["state"], "discharging");
    // in seconds
    assert_eq!(json["time_to_empty"], 90.5);
    assert!(json["time_to_full"].is_null());
    assert_eq!(json["energy_full_design"], 60.);
    assert!(json["charge_threshold"].is_null());
  }

  #[test]
  fn session_capabilities() {
    let capabilities = pw::SessionCapabilities {
      suspend: true,
      hibernate: false,
      suspend_then_hibernate: true,
      reboot: true,
      power_off: false,
      reboot_to_firmware: true,
      boot_entries: vec!["arch.conf".into()],
    };
    let json = json(SessionCapabilities::from(capabilities));
    assert_eq!(json["suspend"], true);
    assert_eq!(json["hibernate"], false);
    assert_eq!(json["suspend_then_hibernate"], true);
    assert_eq!(json["power_off"], false);
    assert_eq!(json["reboot_to_firmware"], true);
    assert_eq!(json["boot_entries"][0], "arch.conf");
  }

  #[test]
  fn session_actions() {
    let all = [
      (SessionAction::Logout, "logout", pw::SessionAction::Logout),
      (
        SessionAction::Suspend,
        "suspend",
        pw::SessionAction::Suspend,
      ),
      (
        SessionAction::Hibernate,
        "hibernate",
        pw::SessionAction::Hibernate,
      ),
      (
        SessionAction::SuspendThenHibernate,
        "suspend_then_hibernate",
        pw::SessionAction::SuspendThenHibernate,
      ),
      (SessionAction::Reboot, "reboot", pw::SessionAction::Reboot),
      (
        SessionAction::PowerOff,
        "power_off",
        pw::SessionAction::PowerOff,
      ),
      (
        SessionAction::RebootToFirmware,
        "reboot_to_firmware",
        pw::SessionAction::RebootToFirmware,
      ),
    ];
    for (action, name, session) in all {
      assert_eq!(serde_json::to_value(action).unwrap(), name);
      assert_eq!(
        serde_json::from_value::<SessionAction>(name.into()).unwrap(),
        action
      );
      assert_eq!(pw::SessionAction::from(action), session);
    }
  }
}
