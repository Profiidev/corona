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
  let device = bluetooth
    .list_devices(cx)
    .iter()
    .find(|d| d.path.as_str() == request.device);
  let (kind, passkey) = match request.kind {
    bt::PairingKind::Confirm { passkey } => (PairingKind::Confirm, Some(passkey)),
    bt::PairingKind::Authorize => (PairingKind::Authorize, None),
    bt::PairingKind::PinCode => (PairingKind::PinCode, None),
    bt::PairingKind::Passkey => (PairingKind::Passkey, None),
    bt::PairingKind::DisplayPasskey { passkey } => (PairingKind::DisplayPasskey, Some(passkey)),
  };
  Some(PairingRequest {
    address: device.map(|d| d.address.clone()),
    name: device.map_or_else(|| request.device.clone(), |d| d.name.clone()),
    kind,
    passkey: passkey.map(|passkey| format!("{passkey:06}")),
  })
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
        let answer = match (accept, code) {
          (false, _) => bt::PairingAnswer::Reject,
          (true, Some(code)) => bt::PairingAnswer::Code(code),
          (true, None) => bt::PairingAnswer::Accept,
        };
        cx.bluetooth().clone().answer_pairing(cx, answer)
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
