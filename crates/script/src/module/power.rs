use corona_power as pw;
use corona_power::{Power, PowerExt};
use gpui_kit::App;
use gpui_shell::HostModule;
use serde::Serialize;
use ts_rs::TS;

use crate::{
  host_fn::{Cx, Glob, Module},
  module::{Subscribe, Subscriptions, read},
};
use corona_macros::named;

#[derive(Serialize, TS)]
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

#[derive(Serialize, TS)]
struct Status {
  on_battery: bool,
  lid_closed: Option<bool>,
  critical_action: String,
}

#[derive(Serialize, TS)]
struct ChargeThreshold {
  enabled: bool,
  start: u32,
  end: u32,
}

#[derive(Serialize, TS)]
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

#[derive(Serialize, TS)]
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

#[derive(Serialize, TS)]
struct Device {
  model: String,
  kind: DeviceKind,
  percentage: f64,
  state: BatteryState,
}

#[derive(Serialize, TS)]
struct Profiles {
  active: String,
  available: Vec<String>,
  degraded: Option<String>,
}

#[derive(Serialize, TS)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Updates {
  Status,
  Battery,
  Devices,
  Profiles,
  KeyboardBacklight,
}

impl From<Updates> for super::Updates {
  fn from(value: Updates) -> Self {
    super::Updates::Power(value)
  }
}

pub fn module(reads: &Subscriptions, subs: &mut Vec<Subscribe>, cx: &mut App) -> HostModule {
  let state = cx.power();

  Module::new("corona/power")
    .func(read(
      reads,
      subs,
      "status",
      Updates::Status,
      state.status.clone(),
      |cx| {
        cx.power().status(cx).map(|s| Status {
          on_battery: s.on_battery,
          lid_closed: s.lid_closed,
          critical_action: s.critical_action.clone(),
        })
      },
    ))
    .func(read(
      reads,
      subs,
      "battery",
      Updates::Battery,
      state.battery.clone(),
      |cx| cx.power().battery(cx).map(Battery::from),
    ))
    .func(read(
      reads,
      subs,
      "listDevices",
      Updates::Devices,
      state.devices.clone(),
      |cx| {
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
      },
    ))
    .func(read(
      reads,
      subs,
      "profiles",
      Updates::Profiles,
      state.profiles.clone(),
      |cx| {
        cx.power().profiles(cx).map(|p| Profiles {
          active: p.active.clone(),
          available: p.available.clone(),
          degraded: p.degraded.clone(),
        })
      },
    ))
    .func(read(
      reads,
      subs,
      "keyboardBacklight",
      Updates::KeyboardBacklight,
      state.keyboard_backlight.clone(),
      |cx| {
        cx.power()
          .keyboard_backlight(cx)
          .map(|k| KeyboardBacklight {
            brightness: k.brightness,
            max: k.max,
          })
      },
    ))
    .func(named!(
      "setProfile",
      /// `power-saver`, `balanced` or `performance`.
      |power: Glob<Power>, profile: String| power.set_profile(profile)
    ))
    .func(named!(
      "setKeyboardBrightness",
      |cx: Cx, power: Glob<Power>, brightness: i32| power.set_keyboard_brightness(brightness, &cx)
    ))
    .func(named!(
      "setChargeThreshold",
      |cx: Cx, power: Glob<Power>, enabled: bool| power.set_charge_threshold(enabled, &cx)
    ))
    .into()
}
