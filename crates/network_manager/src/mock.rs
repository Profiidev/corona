//! A fake NetworkManager on a test bus: one wired and one wifi device, access
//! points, saved profiles and an agent manager, all read from one `World`.

use std::{
  collections::HashMap,
  sync::{Arc, Mutex, MutexGuard},
};

use corona_utils::test_bus::TestBus;
use futures_lite::future::block_on;
use zbus::{
  Connection, interface,
  message::Header,
  object_server::SignalEmitter,
  zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value},
};

pub(crate) const NM: &str = "/org/freedesktop/NetworkManager";
pub(crate) const ETH: &str = "/org/freedesktop/NetworkManager/Devices/1";
pub(crate) const WLAN: &str = "/org/freedesktop/NetworkManager/Devices/2";
pub(crate) const IP4: &str = "/org/freedesktop/NetworkManager/IP4Config/1";
pub(crate) const SETTINGS: &str = "/org/freedesktop/NetworkManager/Settings";

pub(crate) type SettingsMap = HashMap<String, HashMap<String, OwnedValue>>;

#[derive(Clone)]
pub(crate) struct Ap {
  pub path: String,
  pub ssid: Vec<u8>,
  pub strength: u8,
  pub flags: u32,
  pub wpa: u32,
  pub rsn: u32,
}

#[derive(Clone)]
pub(crate) struct Profile {
  pub path: String,
  pub id: String,
  pub uuid: String,
  pub kind: String,
  pub ssid: Option<Vec<u8>>,
}

#[derive(Clone)]
pub(crate) struct Active {
  pub path: String,
  pub uuid: String,
  pub state: u32,
  pub devices: Vec<String>,
}

pub(crate) struct World {
  pub calls: Vec<String>,
  /// the settings of every AddAndActivateConnection, in order
  pub added: Vec<SettingsMap>,
  pub wifi_enabled: bool,
  pub connectivity: u32,
  pub check_enabled: bool,
  pub check_uri: String,
  pub primary: String,
  pub eth_state: u32,
  pub wlan_state: u32,
  pub wlan_type: u32,
  pub active_ap: String,
  pub aps: Vec<Ap>,
  pub last_scan: i64,
  /// RequestScan finishes the scan right away
  pub scan_finishes: bool,
  pub profiles: Vec<Profile>,
  /// profiles the wifi device may use, by path
  pub available: Vec<String>,
  pub active: Vec<Active>,
  /// who registered a secret agent: (sender, identifier)
  pub agent: Option<(String, String)>,
}

pub(crate) fn ap(n: u32, ssid: &[u8], strength: u8, rsn: u32) -> Ap {
  Ap {
    path: format!("{NM}/AccessPoint/{n}"),
    ssid: ssid.to_vec(),
    strength,
    flags: 0,
    wpa: 0,
    rsn,
  }
}

pub(crate) fn profile(n: u32, id: &str, kind: &str, ssid: Option<&[u8]>) -> Profile {
  Profile {
    path: format!("{SETTINGS}/{n}"),
    id: id.into(),
    uuid: format!("uuid-{id}"),
    kind: kind.into(),
    ssid: ssid.map(<[u8]>::to_vec),
  }
}

impl Default for World {
  /// wired and connected through eth0, wifi on wlan0 near home (saved) and cafe
  fn default() -> Self {
    let psk = 0x100;
    Self {
      calls: Vec::new(),
      added: Vec::new(),
      wifi_enabled: true,
      connectivity: 4,
      check_enabled: true,
      check_uri: "http://check.example/".into(),
      primary: format!("{NM}/ActiveConnection/1"),
      eth_state: 100,
      wlan_state: 30,
      wlan_type: 2,
      active_ap: "/".into(),
      aps: vec![
        ap(1, b"home", 40, psk),
        ap(2, b"cafe", 70, 0),
        ap(3, b"home", 80, psk),
      ],
      last_scan: 1000,
      scan_finishes: true,
      profiles: vec![
        profile(1, "Wired", "802-3-ethernet", None),
        profile(2, "home", "802-11-wireless", Some(b"home")),
        profile(3, "Office VPN", "vpn", None),
        profile(4, "wg0", "wireguard", None),
      ],
      available: vec![format!("{SETTINGS}/2")],
      active: vec![
        Active {
          path: format!("{NM}/ActiveConnection/1"),
          uuid: "uuid-Wired".into(),
          state: 2,
          devices: vec![ETH.into()],
        },
        Active {
          path: format!("{NM}/ActiveConnection/2"),
          uuid: "uuid-wg0".into(),
          state: 1,
          devices: vec![],
        },
      ],
      agent: None,
    }
  }
}

pub(crate) type Shared = Arc<Mutex<World>>;

fn path(p: &str) -> OwnedObjectPath {
  OwnedObjectPath::try_from(p).unwrap()
}

fn paths<'a>(list: impl IntoIterator<Item = &'a String>) -> Vec<OwnedObjectPath> {
  list.into_iter().map(|p| path(p)).collect()
}

fn owned(value: Value<'_>) -> OwnedValue {
  value.try_to_owned().unwrap()
}

struct Root(Shared);

impl Root {
  fn world(&self) -> MutexGuard<'_, World> {
    self.0.lock().unwrap()
  }
}

#[interface(name = "org.freedesktop.NetworkManager")]
impl Root {
  fn activate_connection(
    &self,
    connection: ObjectPath<'_>,
    device: ObjectPath<'_>,
    specific: ObjectPath<'_>,
  ) -> OwnedObjectPath {
    self.world().calls.push(format!(
      "ActivateConnection {connection} {device} {specific}"
    ));
    path(&format!("{NM}/ActiveConnection/1"))
  }

  fn add_and_activate_connection(
    &self,
    connection: SettingsMap,
    device: ObjectPath<'_>,
    specific: ObjectPath<'_>,
  ) -> (OwnedObjectPath, OwnedObjectPath) {
    let mut world = self.world();
    world
      .calls
      .push(format!("AddAndActivateConnection {device} {specific}"));
    world.added.push(connection);
    (
      path(&format!("{SETTINGS}/9")),
      path(&format!("{NM}/ActiveConnection/1")),
    )
  }

  fn deactivate_connection(&self, active: ObjectPath<'_>) {
    self
      .world()
      .calls
      .push(format!("DeactivateConnection {active}"));
  }

  fn get_all_devices(&self) -> Vec<OwnedObjectPath> {
    vec![path(ETH), path(WLAN)]
  }

  fn get_devices(&self) -> Vec<OwnedObjectPath> {
    vec![path(ETH), path(WLAN)]
  }

  fn check_connectivity(&self) -> u32 {
    self.world().calls.push("CheckConnectivity".into());
    2
  }

  #[zbus(property)]
  fn devices(&self) -> Vec<OwnedObjectPath> {
    vec![path(ETH), path(WLAN)]
  }
  #[zbus(property)]
  fn primary_connection(&self) -> OwnedObjectPath {
    path(&self.world().primary)
  }
  #[zbus(property)]
  fn active_connections(&self) -> Vec<OwnedObjectPath> {
    paths(self.world().active.iter().map(|a| &a.path))
  }
  #[zbus(property)]
  fn connectivity(&self) -> u32 {
    self.world().connectivity
  }
  #[zbus(property)]
  fn connectivity_check_enabled(&self) -> bool {
    self.world().check_enabled
  }
  #[zbus(property)]
  fn connectivity_check_available(&self) -> bool {
    true
  }
  #[zbus(property)]
  fn connectivity_check_uri(&self) -> String {
    self.world().check_uri.clone()
  }
  #[zbus(property)]
  fn wireless_enabled(&self) -> bool {
    self.world().wifi_enabled
  }
  #[zbus(property)]
  fn set_wireless_enabled(&mut self, enabled: bool) {
    let mut world = self.world();
    world.wifi_enabled = enabled;
    world.calls.push(format!("WirelessEnabled {enabled}"));
  }
}

struct Device {
  world: Shared,
  path: &'static str,
}

#[interface(name = "org.freedesktop.NetworkManager.Device")]
impl Device {
  fn disconnect(&self) {
    self
      .world
      .lock()
      .unwrap()
      .calls
      .push(format!("Disconnect {}", self.path));
  }
  #[zbus(signal, name = "StateChanged")]
  pub(crate) async fn device_state_changed(
    emitter: &SignalEmitter<'_>,
    new: u32,
    old: u32,
    reason: u32,
  ) -> zbus::Result<()>;

  #[zbus(property)]
  fn device_type(&self) -> u32 {
    match self.path {
      ETH => 1,
      _ => self.world.lock().unwrap().wlan_type,
    }
  }
  #[zbus(property)]
  fn interface(&self) -> String {
    match self.path {
      ETH => "eth0".into(),
      _ => "wlan0".into(),
    }
  }
  #[zbus(property)]
  fn state(&self) -> u32 {
    let world = self.world.lock().unwrap();
    match self.path {
      ETH => world.eth_state,
      _ => world.wlan_state,
    }
  }
  #[zbus(property)]
  fn ip4_config(&self) -> OwnedObjectPath {
    match self.path {
      ETH => path(IP4),
      _ => path("/"),
    }
  }
  #[zbus(property)]
  fn available_connections(&self) -> Vec<OwnedObjectPath> {
    match self.path {
      ETH => vec![path(&format!("{SETTINGS}/1"))],
      _ => paths(&self.world.lock().unwrap().available),
    }
  }
}

struct Wireless(Shared);

#[interface(name = "org.freedesktop.NetworkManager.Device.Wireless")]
impl Wireless {
  fn get_access_points(&self) -> Vec<OwnedObjectPath> {
    paths(self.0.lock().unwrap().aps.iter().map(|a| &a.path))
  }

  async fn request_scan(
    &self,
    _options: HashMap<String, OwnedValue>,
    #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
  ) -> zbus::fdo::Result<()> {
    let finishes = {
      let mut world = self.0.lock().unwrap();
      world.calls.push("RequestScan".into());
      if world.scan_finishes {
        world.last_scan += 1;
      }
      world.scan_finishes
    };
    if finishes {
      self.last_scan_changed(&emitter).await?;
    }
    Ok(())
  }

  #[zbus(property)]
  fn active_access_point(&self) -> OwnedObjectPath {
    path(&self.0.lock().unwrap().active_ap)
  }
  #[zbus(property)]
  fn last_scan(&self) -> i64 {
    self.0.lock().unwrap().last_scan
  }
}

struct AccessPoint {
  world: Shared,
  path: String,
}

impl AccessPoint {
  fn ap(&self) -> Ap {
    let world = self.world.lock().unwrap();
    world
      .aps
      .iter()
      .find(|a| a.path == self.path)
      .cloned()
      .expect("a listed access point")
  }
}

#[interface(name = "org.freedesktop.NetworkManager.AccessPoint")]
impl AccessPoint {
  #[zbus(property)]
  fn ssid(&self) -> Vec<u8> {
    self.ap().ssid
  }
  #[zbus(property)]
  fn strength(&self) -> u8 {
    self.ap().strength
  }
  #[zbus(property)]
  fn flags(&self) -> u32 {
    self.ap().flags
  }
  #[zbus(property)]
  fn wpa_flags(&self) -> u32 {
    self.ap().wpa
  }
  #[zbus(property)]
  fn rsn_flags(&self) -> u32 {
    self.ap().rsn
  }
}

struct Ip4;

#[interface(name = "org.freedesktop.NetworkManager.IP4Config")]
impl Ip4 {
  #[zbus(property)]
  fn address_data(&self) -> Vec<HashMap<String, OwnedValue>> {
    vec![HashMap::from([
      ("address".to_string(), owned("192.168.1.5".into())),
      ("prefix".to_string(), owned(24u32.into())),
    ])]
  }
}

struct ActiveConnection {
  world: Shared,
  path: String,
}

impl ActiveConnection {
  fn active(&self) -> Option<Active> {
    let world = self.world.lock().unwrap();
    world.active.iter().find(|a| a.path == self.path).cloned()
  }
}

#[interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
impl ActiveConnection {
  #[zbus(property)]
  fn devices(&self) -> Vec<OwnedObjectPath> {
    self.active().map(|a| paths(&a.devices)).unwrap_or_default()
  }
  #[zbus(property)]
  fn uuid(&self) -> String {
    self.active().map(|a| a.uuid).unwrap_or_default()
  }
  #[zbus(property)]
  fn state(&self) -> u32 {
    self.active().map_or(4, |a| a.state)
  }
}

struct Settings(Shared);

#[interface(name = "org.freedesktop.NetworkManager.Settings")]
impl Settings {
  fn list_connections(&self) -> Vec<OwnedObjectPath> {
    paths(self.0.lock().unwrap().profiles.iter().map(|p| &p.path))
  }
  fn get_connection_by_uuid(&self, uuid: String) -> zbus::fdo::Result<OwnedObjectPath> {
    let world = self.0.lock().unwrap();
    let profile = world.profiles.iter().find(|p| p.uuid == uuid);
    profile
      .map(|p| path(&p.path))
      .ok_or_else(|| zbus::fdo::Error::Failed(format!("no connection {uuid}")))
  }
}

struct ProfileObject {
  world: Shared,
  path: String,
}

#[interface(name = "org.freedesktop.NetworkManager.Settings.Connection")]
impl ProfileObject {
  fn get_settings(&self) -> zbus::fdo::Result<SettingsMap> {
    let world = self.world.lock().unwrap();
    let profile = world
      .profiles
      .iter()
      .find(|p| p.path == self.path)
      .ok_or_else(|| zbus::fdo::Error::UnknownObject(self.path.clone()))?;
    let mut settings = HashMap::from([(
      "connection".to_string(),
      HashMap::from([
        ("id".to_string(), owned(profile.id.as_str().into())),
        ("uuid".to_string(), owned(profile.uuid.as_str().into())),
        ("type".to_string(), owned(profile.kind.as_str().into())),
      ]),
    )]);
    if let Some(ssid) = &profile.ssid {
      settings.insert(
        "802-11-wireless".into(),
        HashMap::from([("ssid".to_string(), owned(ssid.clone().into()))]),
      );
    }
    Ok(settings)
  }

  fn delete(&self) {
    let mut world = self.world.lock().unwrap();
    world.calls.push(format!("Delete {}", self.path));
    world.profiles.retain(|p| p.path != self.path);
  }
}

struct AgentManager(Shared);

#[interface(name = "org.freedesktop.NetworkManager.AgentManager")]
impl AgentManager {
  fn register(&self, #[zbus(header)] header: Header<'_>, identifier: String) {
    let sender = header.sender().unwrap().to_string();
    let mut world = self.0.lock().unwrap();
    world.calls.push(format!("Register {identifier}"));
    world.agent = Some((sender, identifier));
  }
}

pub(crate) struct MockNm {
  pub conn: Connection,
  pub world: Shared,
}

impl MockNm {
  pub(crate) fn start(bus: &TestBus, world: World) -> Self {
    let world = Arc::new(Mutex::new(world));
    let conn = block_on(async {
      let conn = bus.conn().await;
      let server = conn.object_server();
      server.at(NM, Root(world.clone())).await.unwrap();
      server
        .at(
          ETH,
          Device {
            world: world.clone(),
            path: ETH,
          },
        )
        .await
        .unwrap();
      server
        .at(
          WLAN,
          Device {
            world: world.clone(),
            path: WLAN,
          },
        )
        .await
        .unwrap();
      server.at(WLAN, Wireless(world.clone())).await.unwrap();
      server.at(IP4, Ip4).await.unwrap();
      server.at(SETTINGS, Settings(world.clone())).await.unwrap();
      server
        .at(
          format!("{NM}/AgentManager").as_str(),
          AgentManager(world.clone()),
        )
        .await
        .unwrap();
      let (aps, profiles, active) = {
        let w = world.lock().unwrap();
        (w.aps.clone(), w.profiles.clone(), w.active.clone())
      };
      for ap in aps {
        server
          .at(
            ap.path.as_str(),
            AccessPoint {
              world: world.clone(),
              path: ap.path.clone(),
            },
          )
          .await
          .unwrap();
      }
      for profile in profiles {
        server
          .at(
            profile.path.as_str(),
            ProfileObject {
              world: world.clone(),
              path: profile.path.clone(),
            },
          )
          .await
          .unwrap();
      }
      for active in active {
        server
          .at(
            active.path.as_str(),
            ActiveConnection {
              world: world.clone(),
              path: active.path.clone(),
            },
          )
          .await
          .unwrap();
      }
      conn
        .request_name("org.freedesktop.NetworkManager")
        .await
        .unwrap();
      conn
    });
    Self { conn, world }
  }

  pub(crate) fn world(&self) -> MutexGuard<'_, World> {
    self.world.lock().unwrap()
  }

  pub(crate) fn calls(&self) -> Vec<String> {
    self.world().calls.clone()
  }

  /// tells listeners the NetworkManager properties changed
  pub(crate) fn changed(&self) {
    block_on(async {
      let iface = self
        .conn
        .object_server()
        .interface::<_, Root>(NM)
        .await
        .unwrap();
      iface
        .get()
        .await
        .wireless_enabled_changed(iface.signal_emitter())
        .await
        .unwrap();
    });
  }

  pub(crate) fn device_state(&self, device: &str, new: u32, old: u32, reason: u32) {
    block_on(async {
      let emitter = SignalEmitter::new(&self.conn, device).unwrap();
      Device::device_state_changed(&emitter, new, old, reason)
        .await
        .unwrap();
    });
  }
}
