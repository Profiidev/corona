use anyhow::{Context, Result};
use bluez_zbus::{adapter1::Adapter1Proxy, device1::Device1Proxy};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{App, AppContext, Entity, Global};
use zbus::{Connection, proxy::CacheProperties, zvariant::OwnedObjectPath};

use crate::{
  agent::Reply,
  listener::{agent_listener, listener, subscribe},
};

pub use crate::{
  agent::{PairingKind, PairingRequest},
  state::{Adapter, Device},
};

mod agent;
mod listener;
mod snapshot;
mod state;

#[derive(Clone)]
pub struct Bluetooth {
  pub adapter: Entity<Option<Adapter>>,
  pub devices: Entity<Vec<Device>>,
  pub pairing_request: Entity<Option<PairingRequest>>,
  conn: Connection,
}

impl Global for Bluetooth {}

pub trait BluetoothExt {
  fn bluetooth(&self) -> &Bluetooth;
}

impl BluetoothExt for App {
  fn bluetooth(&self) -> &Bluetooth {
    self.global::<Bluetooth>()
  }
}

pub enum PairingAnswer {
  Reject,
  Accept,
  Code(String),
}

impl Bluetooth {
  pub fn adapter<'c>(&self, cx: &'c App) -> Option<&'c Adapter> {
    self.adapter.read(cx).as_ref()
  }

  pub fn list_devices<'c>(&self, cx: &'c App) -> &'c [Device] {
    self.devices.read(cx)
  }

  pub fn device<'c>(&self, address: &str, cx: &'c App) -> Option<&'c Device> {
    self.list_devices(cx).iter().find(|d| d.address == address)
  }

  pub fn pairing_request<'c>(&self, cx: &'c App) -> Option<&'c PairingRequest> {
    self.pairing_request.read(cx).as_ref()
  }

  fn adapter_path(&self, cx: &App) -> Result<OwnedObjectPath> {
    Ok(
      self
        .adapter(cx)
        .context("no Bluetooth adapter")?
        .path
        .clone(),
    )
  }

  fn device_path(&self, address: &str, cx: &App) -> Result<OwnedObjectPath> {
    Ok(
      self
        .device(address, cx)
        .with_context(|| format!("no device {address}"))?
        .path
        .clone(),
    )
  }

  pub fn set_powered(&self, powered: bool, cx: &App) -> impl Future<Output = Result<()>> + use<> {
    let (conn, path) = (self.conn.clone(), self.adapter_path(cx));
    async move { Ok(adapter(&conn, path?).await?.set_powered(powered).await?) }
  }

  pub fn set_discoverable(
    &self,
    discoverable: bool,
    cx: &App,
  ) -> impl Future<Output = Result<()>> + use<> {
    let (conn, path) = (self.conn.clone(), self.adapter_path(cx));
    async move {
      Ok(
        adapter(&conn, path?)
          .await?
          .set_discoverable(discoverable)
          .await?,
      )
    }
  }

  pub fn start_discovery(&self, cx: &App) -> impl Future<Output = Result<()>> + use<> {
    let (conn, path) = (self.conn.clone(), self.adapter_path(cx));
    async move { Ok(adapter(&conn, path?).await?.start_discovery().await?) }
  }

  pub fn stop_discovery(&self, cx: &App) -> impl Future<Output = Result<()>> + use<> {
    let (conn, path) = (self.conn.clone(), self.adapter_path(cx));
    async move { Ok(adapter(&conn, path?).await?.stop_discovery().await?) }
  }

  pub fn connect(&self, address: &str, cx: &App) -> impl Future<Output = Result<()>> + use<> {
    let (conn, path) = (self.conn.clone(), self.device_path(address, cx));
    async move { Ok(device(&conn, path?).await?.connect().await?) }
  }

  pub fn disconnect(&self, address: &str, cx: &App) -> impl Future<Output = Result<()>> + use<> {
    let (conn, path) = (self.conn.clone(), self.device_path(address, cx));
    async move { Ok(device(&conn, path?).await?.disconnect().await?) }
  }

  pub fn pair(&self, address: &str, cx: &App) -> impl Future<Output = Result<()>> + use<> {
    let (conn, path) = (self.conn.clone(), self.device_path(address, cx));
    async move {
      let device = device(&conn, path?).await?;
      device.pair().await?;
      device.set_trusted(true).await?;
      Ok(device.connect().await?)
    }
  }

  pub fn forget(&self, address: &str, cx: &App) -> impl Future<Output = Result<()>> + use<> {
    let conn = self.conn.clone();
    let paths = self.adapter_path(cx).and_then(|adapter| {
      let device = self.device_path(address, cx)?;
      Ok((adapter, device))
    });
    async move {
      let (adapter_path, device_path) = paths?;
      Ok(
        adapter(&conn, adapter_path)
          .await?
          .remove_device(&device_path)
          .await?,
      )
    }
  }

  pub fn answer_pairing(&self, cx: &mut App, answer: PairingAnswer) {
    let request = self.pairing_request.update(cx, |request, cx| {
      cx.notify();
      request.take()
    });
    let Some(request) = request else {
      return;
    };
    let accept = !matches!(answer, PairingAnswer::Reject);
    let code = match answer {
      PairingAnswer::Code(code) => Some(code.trim().to_string()),
      _ => None,
    };
    match request.reply {
      Reply::Accept(reply) => _ = reply.send(accept),
      Reply::PinCode(reply) => _ = reply.send(code.filter(|_| accept)),
      Reply::Passkey(reply) => _ = reply.send(code.and_then(|code| code.parse().ok())),
      Reply::None => {}
    }
  }
}

async fn adapter(conn: &Connection, path: OwnedObjectPath) -> zbus::Result<Adapter1Proxy<'static>> {
  Adapter1Proxy::builder(conn)
    .path(path)?
    .cache_properties(CacheProperties::No)
    .build()
    .await
}

async fn device(conn: &Connection, path: OwnedObjectPath) -> zbus::Result<Device1Proxy<'static>> {
  Device1Proxy::builder(conn)
    .path(path)?
    .cache_properties(CacheProperties::No)
    .build()
    .await
}

pub async fn init(cx: &mut App, conn: &Connection) -> Result<()> {
  let state = Bluetooth {
    adapter: cx.new(|_| None),
    devices: cx.new(|_| Vec::new()),
    pairing_request: cx.new(|_| None),
    conn: conn.clone(),
  };

  // subscribe before the first snapshot so no change can slip in between
  let changes = subscribe(conn).await?;
  listener(cx, conn.clone(), changes, state.clone());
  if let Ok(messages) = agent::register(conn).await.log_err() {
    agent_listener(cx, messages, state.pairing_request.clone());
  }
  cx.set_global(state);

  Ok(())
}

#[cfg(test)]
mod tests {
  use std::{
    sync::{Arc, Mutex},
    thread,
  };

  use corona_utils::test_bus::{TestBus, settle, wait_until};
  use futures_lite::future::block_on;
  use gpui_kit::{self as gpui, TestAppContext};
  use zbus::{
    fdo::ObjectManager, interface, message::Header, object_server::SignalEmitter,
    zvariant::ObjectPath,
  };

  use super::*;

  type Calls = Arc<Mutex<Vec<String>>>;

  struct MockAdapter {
    calls: Calls,
    powered: bool,
    discoverable: bool,
  }

  #[interface(name = "org.bluez.Adapter1")]
  impl MockAdapter {
    fn start_discovery(&self) {
      self.calls.lock().unwrap().push("StartDiscovery".into());
    }
    fn stop_discovery(&self) {
      self.calls.lock().unwrap().push("StopDiscovery".into());
    }
    fn remove_device(&self, device: ObjectPath<'_>) {
      self
        .calls
        .lock()
        .unwrap()
        .push(format!("RemoveDevice {device}"));
    }
    #[zbus(property)]
    fn alias(&self) -> String {
      "Laptop".into()
    }
    #[zbus(property)]
    fn powered(&self) -> bool {
      self.powered
    }
    #[zbus(property)]
    fn set_powered(&mut self, powered: bool) {
      self.powered = powered;
      self
        .calls
        .lock()
        .unwrap()
        .push(format!("Powered {powered}"));
    }
    #[zbus(property)]
    fn discoverable(&self) -> bool {
      self.discoverable
    }
    #[zbus(property)]
    fn set_discoverable(&mut self, discoverable: bool) {
      self.discoverable = discoverable;
      self
        .calls
        .lock()
        .unwrap()
        .push(format!("Discoverable {discoverable}"));
    }
  }

  struct MockDevice {
    calls: Calls,
    address: String,
    connected: bool,
    trusted: bool,
  }

  #[interface(name = "org.bluez.Device1")]
  impl MockDevice {
    async fn connect(&mut self, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>) {
      self
        .calls
        .lock()
        .unwrap()
        .push(format!("Connect {}", self.address));
      self.connected = true;
      self.connected_changed(&emitter).await.unwrap();
    }
    fn disconnect(&self) {
      self
        .calls
        .lock()
        .unwrap()
        .push(format!("Disconnect {}", self.address));
    }
    fn pair(&self) {
      self
        .calls
        .lock()
        .unwrap()
        .push(format!("Pair {}", self.address));
    }
    #[zbus(property)]
    fn address(&self) -> String {
      self.address.clone()
    }
    #[zbus(property)]
    fn adapter(&self) -> ObjectPath<'static> {
      ObjectPath::from_static_str_unchecked("/org/bluez/hci0")
    }
    #[zbus(property)]
    fn connected(&self) -> bool {
      self.connected
    }
    #[zbus(property)]
    fn trusted(&self) -> bool {
      self.trusted
    }
    #[zbus(property)]
    fn set_trusted(&mut self, trusted: bool) {
      self.trusted = trusted;
      self
        .calls
        .lock()
        .unwrap()
        .push(format!("Trusted {trusted}"));
    }
    #[zbus(property, name = "RSSI")]
    fn rssi(&self) -> i16 {
      -50
    }
  }

  struct Battery;

  #[interface(name = "org.bluez.Battery1")]
  impl Battery {
    #[zbus(property)]
    fn percentage(&self) -> u8 {
      64
    }
  }

  /// who registered the agent, as (sender, path)
  type Agent = Arc<Mutex<Option<(String, String)>>>;

  struct Manager {
    calls: Calls,
    agent: Agent,
  }

  #[interface(name = "org.bluez.AgentManager1")]
  impl Manager {
    fn register_agent(
      &self,
      #[zbus(header)] header: Header<'_>,
      agent: ObjectPath<'_>,
      capability: String,
    ) {
      let sender = header.sender().unwrap().to_string();
      *self.agent.lock().unwrap() = Some((sender, agent.to_string()));
      self
        .calls
        .lock()
        .unwrap()
        .push(format!("RegisterAgent {agent} {capability}"));
    }
    fn request_default_agent(&self, agent: ObjectPath<'_>) {
      self
        .calls
        .lock()
        .unwrap()
        .push(format!("RequestDefaultAgent {agent}"));
    }
  }

  struct Bluez {
    conn: Connection,
    calls: Calls,
    agent: Agent,
  }

  const HCI0: &str = "/org/bluez/hci0";

  impl Bluez {
    fn start(bus: &TestBus) -> Self {
      let calls = Calls::default();
      let agent = Agent::default();
      let conn = block_on(async {
        let conn = bus.conn().await;
        let server = conn.object_server();
        server.at("/", ObjectManager).await.unwrap();
        server
          .at(
            "/org/bluez",
            Manager {
              calls: calls.clone(),
              agent: agent.clone(),
            },
          )
          .await
          .unwrap();
        server
          .at(
            HCI0,
            MockAdapter {
              calls: calls.clone(),
              powered: true,
              discoverable: false,
            },
          )
          .await
          .unwrap();
        conn.request_name("org.bluez").await.unwrap();
        conn
      });
      let bluez = Self { conn, calls, agent };
      bluez.add_device("AA", true);
      bluez
    }

    fn add_device(&self, address: &str, battery: bool) {
      let path = format!("{HCI0}/dev_{address}");
      block_on(async {
        let server = self.conn.object_server();
        let device = MockDevice {
          calls: self.calls.clone(),
          address: address.into(),
          connected: false,
          trusted: false,
        };
        server.at(path.as_str(), device).await.unwrap();
        if battery {
          server.at(path.as_str(), Battery).await.unwrap();
        }
      });
    }

    fn remove_device(&self, address: &str) {
      let path = format!("{HCI0}/dev_{address}");
      block_on(
        self
          .conn
          .object_server()
          .remove::<MockDevice, _>(path.as_str()),
      )
      .unwrap();
    }

    fn calls(&self) -> Vec<String> {
      self.calls.lock().unwrap().clone()
    }

    /// asks corona's agent, as bluetoothd would; the reply comes once answered
    fn ask(
      &self,
      method: &'static str,
      body: impl zbus::export::serde::Serialize + zbus::zvariant::DynamicType + Send + 'static,
    ) -> thread::JoinHandle<zbus::Result<String>> {
      let (sender, path) = self
        .agent
        .lock()
        .unwrap()
        .clone()
        .expect("an agent registered");
      let conn = self.conn.clone();
      thread::spawn(move || {
        block_on(async {
          let reply = conn
            .call_method(
              Some(sender.as_str()),
              path.as_str(),
              Some("org.bluez.Agent1"),
              method,
              &body,
            )
            .await?;
          let body = reply.body();
          Ok(match body.signature().to_string().as_str() {
            "u" => body.deserialize::<u32>()?.to_string(),
            "s" => body.deserialize::<String>()?,
            _ => String::new(),
          })
        })
      })
    }
  }

  fn start(cx: &mut TestAppContext) -> (TestBus, Option<Bluez>) {
    start_with(cx, true)
  }

  fn start_with(cx: &mut TestAppContext, with_bluez: bool) -> (TestBus, Option<Bluez>) {
    cx.executor().allow_parking();
    let bus = TestBus::new();
    let bluez = with_bluez.then(|| Bluez::start(&bus));
    let conn = block_on(bus.conn());
    cx.update(|cx| {
      cx.foreground_executor()
        .clone()
        .block_on(init(cx, &conn))
        .unwrap()
    });
    (bus, bluez)
  }

  fn addresses(cx: &mut TestAppContext) -> Vec<String> {
    cx.read(|cx| {
      cx.bluetooth()
        .list_devices(cx)
        .iter()
        .map(|d| d.address.clone())
        .collect()
    })
  }

  fn dev(address: &str) -> String {
    format!("{HCI0}/dev_{address}")
  }

  #[gpui::test]
  fn reads_and_follows_bluez(cx: &mut TestAppContext) {
    let (_bus, bluez) = start(cx);
    let bluez = bluez.unwrap();
    wait_until(cx, |cx| addresses(cx) == ["AA"]);
    cx.read(|cx| {
      let bluetooth = cx.bluetooth();
      let adapter = bluetooth.adapter(cx).unwrap();
      assert_eq!((adapter.name.as_str(), adapter.powered), ("Laptop", true));
      let device = bluetooth.device("AA", cx).unwrap();
      assert_eq!((device.battery, device.rssi), (Some(64), Some(-50)));
      assert!(bluetooth.device("nope", cx).is_none());
      assert!(bluetooth.pairing_request(cx).is_none());
    });
    bluez.add_device("BB", false);
    wait_until(cx, |cx| addresses(cx).len() == 2);
    bluez.remove_device("AA");
    wait_until(cx, |cx| addresses(cx) == ["BB"]);
  }

  #[gpui::test]
  #[ignore = "BUG: the listener only hears signals sent by org.bluez, a bluetoothd that quits or crashes leaves the old adapter and devices on screen"]
  fn bug_bluetoothd_quitting_is_noticed(cx: &mut TestAppContext) {
    let (_bus, bluez) = start(cx);
    wait_until(cx, |cx| addresses(cx) == ["AA"]);
    drop(bluez);
    wait_until(cx, |cx| cx.read(|cx| cx.bluetooth().adapter(cx).is_none()));
  }

  type Action = dyn Fn(&Bluetooth, &App) -> std::pin::Pin<Box<dyn Future<Output = Result<()>>>>;

  #[gpui::test]
  fn actions_reach_bluez(cx: &mut TestAppContext) {
    let (_bus, bluez) = start(cx);
    let bluez = bluez.unwrap();
    wait_until(cx, |cx| addresses(cx) == ["AA"]);
    let run = |cx: &mut TestAppContext, f: &Action| {
      let task = cx.read(|cx| f(cx.bluetooth(), cx));
      block_on(task)
    };
    run(cx, &|b, cx| Box::pin(b.set_powered(false, cx))).unwrap();
    run(cx, &|b, cx| Box::pin(b.set_discoverable(true, cx))).unwrap();
    run(cx, &|b, cx| Box::pin(b.start_discovery(cx))).unwrap();
    run(cx, &|b, cx| Box::pin(b.stop_discovery(cx))).unwrap();
    run(cx, &|b, cx| Box::pin(b.disconnect("AA", cx))).unwrap();
    run(cx, &|b, cx| Box::pin(b.pair("AA", cx))).unwrap();
    run(cx, &|b, cx| Box::pin(b.forget("AA", cx))).unwrap();
    let calls: Vec<_> = bluez
      .calls()
      .into_iter()
      .filter(|c| !c.starts_with("Register") && !c.starts_with("RequestDefault"))
      .collect();
    assert_eq!(
      calls,
      [
        "Powered false".to_string(),
        "Discoverable true".into(),
        "StartDiscovery".into(),
        "StopDiscovery".into(),
        "Disconnect AA".into(),
        // pairing trusts and connects
        "Pair AA".into(),
        "Trusted true".into(),
        "Connect AA".into(),
        format!("RemoveDevice {}", dev("AA")),
      ]
    );
    // the connect shows up
    wait_until(cx, |cx| {
      cx.read(|cx| cx.bluetooth().device("AA", cx).unwrap().connected)
    });

    let error = run(cx, &|b, cx| Box::pin(b.connect("ZZ", cx))).unwrap_err();
    assert_eq!(error.to_string(), "no device ZZ");
    assert_eq!(
      run(cx, &|b, cx| Box::pin(b.forget("ZZ", cx)))
        .unwrap_err()
        .to_string(),
      "no device ZZ"
    );
  }

  #[gpui::test]
  fn without_bluez(cx: &mut TestAppContext) {
    let (_bus, _) = start_with(cx, false);
    settle(cx);
    cx.read(|cx| {
      let bluetooth = cx.bluetooth();
      assert!(bluetooth.adapter(cx).is_none());
      assert_eq!(
        block_on(bluetooth.set_powered(true, cx))
          .unwrap_err()
          .to_string(),
        "no Bluetooth adapter"
      );
      assert_eq!(
        block_on(bluetooth.start_discovery(cx))
          .unwrap_err()
          .to_string(),
        "no Bluetooth adapter"
      );
      assert_eq!(
        block_on(bluetooth.forget("AA", cx))
          .unwrap_err()
          .to_string(),
        "no Bluetooth adapter"
      );
    });
  }

  #[gpui::test]
  fn registers_its_agent(cx: &mut TestAppContext) {
    let (_bus, bluez) = start(cx);
    let bluez = bluez.unwrap();
    wait_until(cx, |cx| addresses(cx).len() == 1);
    let calls = bluez.calls();
    assert_eq!(
      calls[..2],
      [
        "RegisterAgent /io/corona/bluez/agent KeyboardDisplay".to_string(),
        "RequestDefaultAgent /io/corona/bluez/agent".into(),
      ]
    );
  }

  fn answer(cx: &mut TestAppContext, answer: PairingAnswer) {
    cx.update(|cx| cx.bluetooth().clone().answer_pairing(cx, answer));
  }

  fn pending(cx: &mut TestAppContext) -> Option<(String, PairingKind)> {
    cx.read(|cx| {
      cx.bluetooth()
        .pairing_request(cx)
        .map(|r| (r.device.clone(), r.kind))
    })
  }

  #[gpui::test]
  fn confirming_a_pairing(cx: &mut TestAppContext) {
    let (_bus, bluez) = start(cx);
    let bluez = bluez.unwrap();
    wait_until(cx, |cx| addresses(cx).len() == 1);
    let device = ObjectPath::try_from(dev("AA")).unwrap().into_owned();

    let reply = bluez.ask("RequestConfirmation", (device.clone(), 123456u32));
    wait_until(cx, |cx| pending(cx).is_some());
    assert_eq!(
      pending(cx),
      Some((dev("AA"), PairingKind::Confirm { passkey: 123456 }))
    );
    answer(cx, PairingAnswer::Accept);
    assert!(pending(cx).is_none());
    assert!(reply.join().unwrap().is_ok());

    let reply = bluez.ask("RequestAuthorization", (device,));
    wait_until(cx, |cx| pending(cx).is_some());
    answer(cx, PairingAnswer::Reject);
    assert!(reply.join().unwrap().is_err());
  }

  #[gpui::test]
  fn entering_codes(cx: &mut TestAppContext) {
    let (_bus, bluez) = start(cx);
    let bluez = bluez.unwrap();
    wait_until(cx, |cx| addresses(cx).len() == 1);
    let device = || ObjectPath::try_from(dev("AA")).unwrap().into_owned();

    let reply = bluez.ask("RequestPasskey", (device(),));
    wait_until(cx, |cx| pending(cx).is_some());
    answer(cx, PairingAnswer::Code(" 012345 ".into()));
    assert_eq!(reply.join().unwrap().unwrap(), "12345");

    // not a number
    let reply = bluez.ask("RequestPasskey", (device(),));
    wait_until(cx, |cx| pending(cx).is_some());
    answer(cx, PairingAnswer::Code("abc".into()));
    assert!(reply.join().unwrap().is_err());

    let reply = bluez.ask("RequestPinCode", (device(),));
    wait_until(cx, |cx| pending(cx).is_some());
    assert_eq!(pending(cx).unwrap().1, PairingKind::PinCode);
    answer(cx, PairingAnswer::Code("  0000 ".into()));
    assert_eq!(reply.join().unwrap().unwrap(), "0000");

    // accepting without a code sends none
    let reply = bluez.ask("RequestPinCode", (device(),));
    wait_until(cx, |cx| pending(cx).is_some());
    answer(cx, PairingAnswer::Accept);
    assert!(reply.join().unwrap().is_err());

    // rejecting with a code typed in sends none either
    let reply = bluez.ask("RequestPinCode", (device(),));
    wait_until(cx, |cx| pending(cx).is_some());
    cx.update(|cx| {
      cx.bluetooth()
        .clone()
        .answer_pairing(cx, PairingAnswer::Reject)
    });
    assert!(reply.join().unwrap().is_err());
  }

  #[gpui::test]
  fn displayed_codes_and_cancel(cx: &mut TestAppContext) {
    let (_bus, bluez) = start(cx);
    let bluez = bluez.unwrap();
    wait_until(cx, |cx| addresses(cx).len() == 1);
    let device = ObjectPath::try_from(dev("AA")).unwrap().into_owned();
    bluez
      .ask("DisplayPasskey", (device.clone(), 42u32, 0u16))
      .join()
      .unwrap()
      .unwrap();
    wait_until(cx, |cx| pending(cx).is_some());
    assert_eq!(
      pending(cx).unwrap().1,
      PairingKind::DisplayPasskey { passkey: 42 }
    );
    bluez.ask("Cancel", ()).join().unwrap().unwrap();
    wait_until(cx, |cx| pending(cx).is_none());

    bluez
      .ask("DisplayPinCode", (device, "000777"))
      .join()
      .unwrap()
      .unwrap();
    wait_until(cx, |cx| pending(cx).is_some());
    // nothing to reply to: answering just closes it
    answer(cx, PairingAnswer::Accept);
    assert!(pending(cx).is_none());
    // answering nothing is fine
    answer(cx, PairingAnswer::Accept);
  }

  #[gpui::test]
  #[ignore = "BUG: the agent holds its lock while waiting for an answer, bluetoothd's Cancel only arrives after the user answered"]
  fn bug_cancel_reaches_a_waiting_prompt(cx: &mut TestAppContext) {
    let (_bus, bluez) = start(cx);
    let bluez = bluez.unwrap();
    wait_until(cx, |cx| addresses(cx).len() == 1);
    let device = ObjectPath::try_from(dev("AA")).unwrap().into_owned();
    let _reply = bluez.ask("RequestConfirmation", (device, 1u32));
    wait_until(cx, |cx| pending(cx).is_some());
    let _cancel = bluez.ask("Cancel", ());
    wait_until(cx, |cx| pending(cx).is_none());
  }
}
