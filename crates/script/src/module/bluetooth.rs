use corona_bluez as bt;
use corona_bluez::{Bluetooth, BluetoothExt};
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
struct Adapter {
  name: String,
  powered: bool,
  discoverable: bool,
  discovering: bool,
}

#[derive(Serialize, TS)]
struct Device {
  address: String,
  name: String,
  icon: Option<String>,
  paired: bool,
  connected: bool,
  trusted: bool,
  battery: Option<u8>,
  rssi: Option<i16>,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "snake_case")]
enum PairingKind {
  Confirm,
  Authorize,
  PinCode,
  Passkey,
  DisplayPasskey,
}

#[derive(Serialize, TS)]
struct PairingRequest {
  address: Option<String>,
  name: String,
  kind: PairingKind,
  passkey: Option<String>,
}

impl From<&bt::Adapter> for Adapter {
  fn from(adapter: &bt::Adapter) -> Self {
    Self {
      name: adapter.name.clone(),
      powered: adapter.powered,
      discoverable: adapter.discoverable,
      discovering: adapter.discovering,
    }
  }
}

impl From<&bt::Device> for Device {
  fn from(device: &bt::Device) -> Self {
    Self {
      address: device.address.clone(),
      name: device.name.clone(),
      icon: device.icon.clone(),
      paired: device.paired,
      connected: device.connected,
      trusted: device.trusted,
      battery: device.battery,
      rssi: device.rssi,
    }
  }
}

fn pairing_request(cx: &App) -> Option<PairingRequest> {
  let bluetooth = cx.bluetooth();
  let request = bluetooth.pairing_request(cx)?;
  Some(describe_pairing(
    &request.device,
    request.kind,
    bluetooth.list_devices(cx),
  ))
}

/// The request for the device at the object path `device`, named by the path if it is unknown.
fn describe_pairing(device: &str, kind: bt::PairingKind, devices: &[bt::Device]) -> PairingRequest {
  let path = device;
  let device = devices.iter().find(|d| d.path.as_str() == path);
  let (kind, passkey) = match kind {
    bt::PairingKind::Confirm { passkey } => (PairingKind::Confirm, Some(passkey)),
    bt::PairingKind::Authorize => (PairingKind::Authorize, None),
    bt::PairingKind::PinCode => (PairingKind::PinCode, None),
    bt::PairingKind::Passkey => (PairingKind::Passkey, None),
    bt::PairingKind::DisplayPasskey { passkey } => (PairingKind::DisplayPasskey, Some(passkey)),
  };
  PairingRequest {
    address: device.map(|d| d.address.clone()),
    name: device.map_or_else(|| path.to_string(), |d| d.name.clone()),
    kind,
    passkey: passkey.map(|passkey| format!("{passkey:06}")),
  }
}

/// A code accepts with that code; rejecting ignores it.
fn pairing_answer(accept: bool, code: Option<String>) -> bt::PairingAnswer {
  match (accept, code) {
    (false, _) => bt::PairingAnswer::Reject,
    (true, Some(code)) => bt::PairingAnswer::Code(code),
    (true, None) => bt::PairingAnswer::Accept,
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Updates {
  Adapter,
  Devices,
  PairingRequest,
}

impl From<Updates> for super::Updates {
  fn from(value: Updates) -> Self {
    super::Updates::Bluetooth(value)
  }
}

pub fn module(reads: &Subscriptions, subs: &mut Vec<Subscribe>, cx: &mut App) -> HostModule {
  let state = cx.bluetooth();

  Module::new("corona/bluetooth")
    .func(read(
      reads,
      subs,
      "adapter",
      Updates::Adapter,
      state.adapter.clone(),
      |cx| cx.bluetooth().adapter(cx).map(Adapter::from),
    ))
    .func(read(
      reads,
      subs,
      "listDevices",
      Updates::Devices,
      state.devices.clone(),
      |cx| {
        let devices = cx.bluetooth().list_devices(cx);
        devices.iter().map(Device::from).collect::<Vec<_>>()
      },
    ))
    .func(read(
      reads,
      subs,
      "pairingRequest",
      Updates::PairingRequest,
      state.pairing_request.clone(),
      pairing_request,
    ))
    .func(named!(
      "answerPairing",
      |cx: &mut App, accept: bool, code: Option<String>| {
        cx.bluetooth()
          .clone()
          .answer_pairing(cx, pairing_answer(accept, code))
      }
    ))
    .func(named!(
      "setPowered",
      |cx: Cx, bt: Glob<Bluetooth>, powered: bool| bt.set_powered(powered, &cx)
    ))
    .func(named!(
      "setDiscoverable",
      |cx: Cx, bt: Glob<Bluetooth>, discoverable: bool| bt.set_discoverable(discoverable, &cx)
    ))
    .func(named!("startDiscovery", |cx: Cx, bt: Glob<Bluetooth>| bt
      .start_discovery(&cx)))
    .func(named!("stopDiscovery", |cx: Cx, bt: Glob<Bluetooth>| bt
      .stop_discovery(&cx)))
    .func(named!(
      "connect",
      |cx: Cx, bt: Glob<Bluetooth>, address: String| bt.connect(&address, &cx)
    ))
    .func(named!(
      "disconnect",
      |cx: Cx, bt: Glob<Bluetooth>, address: String| bt.disconnect(&address, &cx)
    ))
    .func(named!("pair", |cx: Cx,
                          bt: Glob<Bluetooth>,
                          address: String| {
      bt.pair(&address, &cx)
    }))
    .func(named!(
      "forget",
      |cx: Cx, bt: Glob<Bluetooth>, address: String| bt.forget(&address, &cx)
    ))
    .into()
}

#[cfg(test)]
mod tests {
  use super::*;

  fn device(path: &str, address: &str, name: &str) -> bt::Device {
    bt::Device {
      path: path.try_into().unwrap(),
      address: address.into(),
      name: name.into(),
      icon: Some("audio-headset".into()),
      paired: true,
      connected: false,
      trusted: true,
      battery: Some(80),
      rssi: None,
    }
  }

  fn json(value: impl Serialize) -> serde_json::Value {
    serde_json::to_value(value).unwrap()
  }

  #[test]
  fn pairing_kinds() {
    let all = [
      (
        bt::PairingKind::Confirm { passkey: 42 },
        "confirm",
        Some("000042"),
      ),
      (bt::PairingKind::Authorize, "authorize", None),
      (bt::PairingKind::PinCode, "pin_code", None),
      (bt::PairingKind::Passkey, "passkey", None),
      (
        bt::PairingKind::DisplayPasskey { passkey: 123_456 },
        "display_passkey",
        Some("123456"),
      ),
    ];
    for (kind, name, passkey) in all {
      let json = json(describe_pairing("/dev", kind, &[]));
      assert_eq!(json["kind"], name);
      assert_eq!(json["passkey"].as_str(), passkey);
    }
  }

  #[test]
  fn pairing_device_lookup() {
    let devices = [
      device("/org/bluez/hci0/dev_1", "00:11", "Headset"),
      device("/org/bluez/hci0/dev_2", "00:22", "Mouse"),
    ];
    let request = describe_pairing(
      "/org/bluez/hci0/dev_2",
      bt::PairingKind::Authorize,
      &devices,
    );
    assert_eq!(request.address.as_deref(), Some("00:22"));
    assert_eq!(request.name, "Mouse");

    // an unknown device is named by its path
    let request = describe_pairing(
      "/org/bluez/hci0/dev_3",
      bt::PairingKind::Authorize,
      &devices,
    );
    assert_eq!(request.address, None);
    assert_eq!(request.name, "/org/bluez/hci0/dev_3");
  }

  #[test]
  fn pairing_answers() {
    assert!(matches!(
      pairing_answer(false, None),
      bt::PairingAnswer::Reject
    ));
    assert!(matches!(
      pairing_answer(false, Some("1234".into())),
      bt::PairingAnswer::Reject
    ));
    assert!(matches!(
      pairing_answer(true, None),
      bt::PairingAnswer::Accept
    ));
    assert!(matches!(
      pairing_answer(true, Some("1234".into())),
      bt::PairingAnswer::Code(code) if code == "1234"
    ));
  }

  #[test]
  fn devices_and_adapters() {
    let converted = json(Device::from(&device(
      "/org/bluez/hci0/dev_1",
      "00:11",
      "Headset",
    )));
    assert_eq!(converted["address"], "00:11");
    assert_eq!(converted["battery"], 80);
    assert!(converted["rssi"].is_null());
    assert!(converted.get("path").is_none());

    let adapter = bt::Adapter {
      path: "/org/bluez/hci0".try_into().unwrap(),
      name: "hci0".into(),
      powered: true,
      discoverable: false,
      discovering: true,
    };
    let converted = json(Adapter::from(&adapter));
    assert_eq!(converted["name"], "hci0");
    assert_eq!(converted["powered"], true);
    assert_eq!(converted["discoverable"], false);
    assert_eq!(converted["discovering"], true);
  }
}
