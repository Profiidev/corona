use corona_bluez as bt;
use corona_bluez::BluetoothExt;
use corona_js::{Cx, Glob, ModuleBuilder, Opt, PluginRef, ScriptModule, Throws, host_module, json};
use gpui_kit::App;
use serde::Serialize;
use specta::{Type, specta};

#[derive(Serialize, Type)]
struct Adapter {
  name: String,
  powered: bool,
  discoverable: bool,
  discovering: bool,
}

#[derive(Serialize, Type)]
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

#[derive(Serialize, Type)]
#[serde(rename_all = "snake_case")]
enum PairingKind {
  Confirm,
  Authorize,
  PinCode,
  Passkey,
  DisplayPasskey,
}

#[derive(Serialize, Type)]
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
    request.kind.clone(),
    bluetooth.list_devices(cx),
  ))
}

/// The request for the device at the object path `device`, named by the path if it is unknown.
fn describe_pairing(device: &str, kind: bt::PairingKind, devices: &[bt::Device]) -> PairingRequest {
  let path = device;
  let device = devices.iter().find(|d| d.path.as_str() == path);
  let (kind, passkey) = match kind {
    bt::PairingKind::Confirm { passkey } => (PairingKind::Confirm, Some(format!("{passkey:06}"))),
    bt::PairingKind::Authorize => (PairingKind::Authorize, None),
    bt::PairingKind::PinCode => (PairingKind::PinCode, None),
    bt::PairingKind::Passkey => (PairingKind::Passkey, None),
    bt::PairingKind::DisplayPasskey { passkey } => {
      (PairingKind::DisplayPasskey, Some(format!("{passkey:06}")))
    }
    // a PIN is shown as it is: it has no fixed length and may not be numeric
    bt::PairingKind::DisplayPinCode { pin } => (PairingKind::DisplayPasskey, Some(pin)),
  };
  PairingRequest {
    address: device.map(|d| d.address.clone()),
    name: device.map_or_else(|| path.to_string(), |d| d.name.clone()),
    kind,
    passkey,
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

type Bt = Glob<bt::Bluetooth>;

/// Accepts or rejects the pairing request, with the code it asks for.
#[specta]
fn answer_pairing(mut cx: Cx, accept: bool, code: Opt<String>) {
  let answer = pairing_answer(accept, code.0);
  cx.bluetooth().clone().answer_pairing(&mut cx, answer)
}

#[specta]
async fn set_powered(cx: Cx, bt: Bt, powered: bool) -> Throws<()> {
  let set = bt.set_powered(powered, &cx);
  set.await.into()
}

#[specta]
async fn set_discoverable(cx: Cx, bt: Bt, discoverable: bool) -> Throws<()> {
  let set = bt.set_discoverable(discoverable, &cx);
  set.await.into()
}

#[specta]
async fn start_discovery(cx: Cx, bt: Bt) -> Throws<()> {
  let start = bt.start_discovery(&cx);
  start.await.into()
}

#[specta]
async fn stop_discovery(cx: Cx, bt: Bt) -> Throws<()> {
  let stop = bt.stop_discovery(&cx);
  stop.await.into()
}

#[specta]
async fn connect(cx: Cx, bt: Bt, address: String) -> Throws<()> {
  let connect = bt.connect(&address, &cx);
  connect.await.into()
}

#[specta]
async fn disconnect(cx: Cx, bt: Bt, address: String) -> Throws<()> {
  let disconnect = bt.disconnect(&address, &cx);
  disconnect.await.into()
}

#[specta]
async fn pair(cx: Cx, bt: Bt, address: String) -> Throws<()> {
  let pair = bt.pair(&address, &cx);
  pair.await.into()
}

#[specta]
async fn forget(cx: Cx, bt: Bt, address: String) -> Throws<()> {
  let forget = bt.forget(&address, &cx);
  forget.await.into()
}

/// `corona/bluetooth`
pub struct BluetoothModule;

impl ScriptModule for BluetoothModule {
  const NAME: &str = "bluetooth";
  const DOC: &str = "Bluetooth adapters and devices, and pairing them.";

  fn module(_: &PluginRef, m: &mut ModuleBuilder, _: &mut App) {
    m.watch(
      "adapter",
      "",
      |cx| cx.bluetooth().adapter.clone(),
      json(|cx| cx.bluetooth().adapter(cx).map(Adapter::from)),
    );
    m.watch(
      "listDevices",
      "",
      |cx| cx.bluetooth().devices.clone(),
      json(|cx| {
        let devices = cx.bluetooth().list_devices(cx);
        devices.iter().map(Device::from).collect::<Vec<_>>()
      }),
    );
    m.watch(
      "pairingRequest",
      "",
      |cx| cx.bluetooth().pairing_request.clone(),
      json(pairing_request),
    );
    host_module!(m, {
      sync: [answer_pairing],
      async: [
        set_powered,
        set_discoverable,
        start_discovery,
        stop_discovery,
        connect,
        disconnect,
        pair,
        forget,
      ],
    });
  }
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
      (
        bt::PairingKind::DisplayPinCode { pin: "1234".into() },
        "display_passkey",
        Some("1234"),
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
