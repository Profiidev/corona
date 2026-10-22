use std::{collections::HashMap, io::Read, path::Path, process::Command, time::Duration};

use anyhow::{Context, Result, bail, ensure};
use cosmic_dbus_networkmanager::{
  active_connection::ActiveConnection,
  device::{Device, wireless::WirelessDevice},
  interface::{
    NetworkManagerProxy,
    access_point::AccessPointProxy,
    active_connection::ActiveConnectionProxy,
    device::{DeviceProxy, wireless::WirelessDeviceProxy},
    enums::NmConnectivityState,
    settings::{SettingsProxy, connection::ConnectionSettingsProxy},
  },
  nm::NetworkManager,
  settings::connection::{Connection as NmConnection, Settings, WifiSecurity, WifiSettings},
};
use futures_lite::{StreamExt, future::or};
use gpui_kit::BackgroundExecutor;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use zbus::{
  Connection,
  proxy::{Builder, CacheProperties, Defaults},
  zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value},
};

use crate::{
  agent::{
    ENTERPRISE_SETTING, IDENTITY_KEY, PASSWORD_KEY, PRIVATE_KEY_PASSWORD_KEY, WIFI_SECURITY_SETTING,
  },
  nl80211,
  snapshot::{AP_MODE, WIFI_SETTING, profile, read_access_point, saved_ssid},
  state::{HotspotBand, HotspotClient},
};

/// the profile `start_hotspot` creates once and updates after
pub(crate) const HOTSPOT_ID: &str = "Corona Hotspot";
/// minutes a hotspot without clients keeps running, unless set otherwise
const AUTO_OFF_DEFAULT: u32 = 10;
// 802-11-wireless-security.pmf values
const PMF_OPTIONAL: i32 = 2;
const PMF_REQUIRED: i32 = 3;
/// the Corona Hotspot's user.data key for its auto-off minutes, 0 for never
const AUTO_OFF_KEY: &str = "corona.auto-off";

// 802-11-wireless-security.key-mgmt values
const KEY_MGMT_PSK: &str = "wpa-psk";
const KEY_MGMT_SAE: &str = "sae";
const KEY_MGMT_EAP: &str = "wpa-eap";

// 802-1x keys and values
const EAP_KEY: &str = "eap";
const EAP_PEAP: &str = "peap";
const EAP_TTLS: &str = "ttls";
const EAP_TLS: &str = "tls";
const PHASE2_KEY: &str = "phase2-auth";
const PHASE2_MSCHAPV2: &str = "mschapv2";
const PHASE2_PAP: &str = "pap";
const ANONYMOUS_IDENTITY_KEY: &str = "anonymous-identity";
const DOMAIN_KEY: &str = "domain-suffix-match";
const CA_CERT_KEY: &str = "ca-cert";
const SYSTEM_CA_KEY: &str = "system-ca-certs";
const CLIENT_CERT_KEY: &str = "client-cert";
const PRIVATE_KEY_KEY: &str = "private-key";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HiddenSecurity {
  Open,
  /// WPA/WPA2 personal, WPA3 transition mode included
  Wpa,
  /// WPA3 personal only
  Wpa3,
}

#[derive(Clone, Debug, Deserialize, TS)]
pub struct EnterpriseConfig {
  pub ssid: String,
  /// the network does not broadcast its SSID
  pub hidden: bool,
  pub eap: EapMethod,
  /// PEAP / TTLS only, None lets the server pick
  pub phase2: Option<Phase2>,
  pub identity: String,
  /// the outer identity sent in the clear, like "anonymous@example.org"
  pub anonymous_identity: Option<String>,
  /// PEAP / TTLS login password, None: NM asks through the secret agent
  pub password: Option<String>,
  /// CA certificate file checked against the server, None trusts the system CA bundle
  pub ca_cert: Option<String>,
  /// the server certificate's domain must end in this, like "example.org"
  pub domain: Option<String>,
  /// TLS only
  pub client_cert: Option<String>,
  /// TLS only
  pub private_key: Option<String>,
  /// TLS only, None: NM asks through the secret agent
  pub private_key_password: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum EapMethod {
  Peap,
  Ttls,
  /// certificate login, no password
  Tls,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Phase2 {
  Mschapv2,
  Pap,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct HotspotConfig {
  pub ssid: String,
  /// WPA2 personal, None: an open hotspot
  pub password: Option<String>,
  pub band: HotspotBand,
  /// WPA3 personal (SAE) instead of WPA2, which radios without it fall back to
  #[serde(default)]
  pub wpa3: bool,
  /// minutes without clients it turns off after, None: never
  #[serde(default = "auto_off_default")]
  pub auto_off: Option<u32>,
  /// keeps the clients from reaching each other
  #[serde(default = "isolate_default")]
  pub isolate: bool,
}

fn isolate_default() -> bool {
  true
}

fn auto_off_default() -> Option<u32> {
  Some(AUTO_OFF_DEFAULT)
}

/// the Corona Hotspot profile's auto-off minutes, None: never, as for any other profile
pub(crate) fn auto_off(settings: &HashMap<String, HashMap<String, OwnedValue>>) -> Option<u32> {
  let value = |setting: &str, key: &str| settings.get(setting)?.get(key)?.try_clone().ok();
  let id = value("connection", "id").and_then(|id| String::try_from(id).ok());
  if id.as_deref() != Some(HOTSPOT_ID) {
    return None;
  }
  let data = value("user", "data").and_then(|data| HashMap::<String, String>::try_from(data).ok());
  match data.as_ref().and_then(|data| data.get(AUTO_OFF_KEY)) {
    Some(minutes) => minutes.parse().ok().filter(|minutes| *minutes > 0),
    // saved before it was settable
    None => auto_off_default(),
  }
}

impl HotspotConfig {
  /// a first hotspot with a random name and password; the hostname would tell everyone around
  /// whose it is
  pub fn generated() -> Result<Self> {
    // no l, o, 0 or 1 to misread, 32 letters for an unbiased pick from a byte
    const LETTERS: &[u8; 32] = b"abcdefghijkmnpqrstuvwxyz23456789";
    let mut random = [0u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut random)?;
    let mut text = random.iter().map(|b| LETTERS[*b as usize % 32] as char);
    Ok(Self {
      ssid: format!("Corona-{}", text.by_ref().take(4).collect::<String>()),
      password: Some(text.collect()),
      band: HotspotBand::Bg,
      wpa3: false,
      auto_off: auto_off_default(),
      isolate: isolate_default(),
    })
  }

  /// what NM rejects, caught before asking it
  pub fn validate(&self) -> Result<()> {
    ensure!(
      (1..=32).contains(&self.ssid.len()),
      "the hotspot name needs 1 to 32 bytes"
    );
    if let Some(password) = &self.password {
      let hex_key = password.len() == 64 && password.bytes().all(|b| b.is_ascii_hexdigit());
      ensure!(
        (8..=63).contains(&password.len()) || hex_key,
        "the hotspot password needs 8 to 63 characters"
      );
    }
    Ok(())
  }
}

async fn proxy<P, T>(conn: &Connection, path: OwnedObjectPath) -> Result<T>
where
  P: From<zbus::Proxy<'static>> + Defaults,
  T: From<P>,
{
  Ok(
    Builder::<P>::new(conn)
      .path(path)?
      .cache_properties(CacheProperties::No)
      .build()
      .await?
      .into(),
  )
}

async fn saved_connections<'d>(device: &Device<'d>, ssid: &[u8]) -> Result<Vec<NmConnection<'d>>> {
  let mut saved = Vec::new();
  for conn in device.available_connections().await? {
    if saved_ssid(&conn).await.ok().flatten().as_deref() == Some(ssid) {
      saved.push(conn);
    }
  }
  Ok(saved)
}

pub(crate) async fn set_wifi_enabled(conn: &Connection, enabled: bool) -> Result<()> {
  let nm = NetworkManager::new(conn).await?;
  nm.set_wireless_enabled(enabled).await?;
  Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanResult {
  Done,
  TimedOut,
}

pub(crate) async fn rescan(
  conn: &Connection,
  device: OwnedObjectPath,
  timeout: impl Future<Output = ()>,
) -> Result<ScanResult> {
  let wireless = Builder::<WirelessDeviceProxy>::new(conn)
    .path(device)?
    .cache_properties(CacheProperties::Yes)
    .build()
    .await?;
  let mut changes = wireless.receive_last_scan_changed().await;
  let before = wireless.last_scan().await?;
  wireless.request_scan(HashMap::new()).await?;

  let done = async {
    while let Some(change) = changes.next().await {
      if change.get().await? > before {
        return Ok(ScanResult::Done);
      }
    }
    bail!("NetworkManager went away while scanning")
  };
  let timed_out = async {
    timeout.await;
    Ok(ScanResult::TimedOut)
  };
  or(done, timed_out).await
}

pub(crate) async fn check_connectivity(conn: &Connection) -> Result<NmConnectivityState> {
  Ok(
    NetworkManager::new(conn)
      .await?
      .check_connectivity()
      .await?,
  )
}

async fn strongest_access_point(
  conn: &Connection,
  device: &OwnedObjectPath,
  ssid: &[u8],
) -> Result<OwnedObjectPath> {
  let wireless = proxy::<WirelessDeviceProxy, WirelessDevice>(conn, device.clone()).await?;
  let mut strongest = None;
  for ap in wireless.get_access_points().await? {
    let Ok(ap_info) = read_access_point(&ap).await else {
      continue;
    };
    if ap_info.ssid == ssid
      && strongest
        .as_ref()
        .is_none_or(|(s, _)| ap_info.strength > *s)
    {
      strongest = Some((ap_info.strength, ap.inner().path().to_owned()));
    }
  }
  let (_, path) =
    strongest.with_context(|| format!("{} is not in range", String::from_utf8_lossy(ssid)))?;
  Ok(path.into())
}

pub(crate) async fn connect_wifi(
  conn: &Connection,
  device: OwnedObjectPath,
  ssid: Vec<u8>,
) -> Result<()> {
  let nm = NetworkManager::new(conn).await?;
  let access_point = strongest_access_point(conn, &device, &ssid).await?;
  let device = proxy::<DeviceProxy, Device>(conn, device).await?;
  let saved = saved_connections(&device, &ssid).await?;
  if let Some(connection) = saved.first() {
    nm.activate_connection(connection, &device).await?;
    return Ok(());
  }

  let settings = Settings {
    wifi: Some(WifiSettings {
      ssid: Some(ssid),
      ..Default::default()
    }),
    ..Default::default()
  };
  nm.add_and_activate_connection(
    borrowed(&settings.build()),
    device.inner().path(),
    &access_point,
  )
  .await?;
  Ok(())
}

type SettingsMap<'a> = HashMap<String, HashMap<String, Value<'a>>>;

fn borrowed<'a>(settings: &'a SettingsMap<'a>) -> HashMap<&'a str, HashMap<&'a str, Value<'a>>> {
  settings
    .iter()
    .map(|(name, setting)| {
      let setting = setting
        .iter()
        .map(|(key, value)| (key.as_str(), value.clone()))
        .collect();
      (name.as_str(), setting)
    })
    .collect()
}

pub(crate) async fn join_hidden_wifi(
  conn: &Connection,
  device: OwnedObjectPath,
  ssid: String,
  security: HiddenSecurity,
  password: Option<String>,
) -> Result<()> {
  let wifi = Settings {
    wifi: Some(WifiSettings {
      ssid: Some(ssid.into_bytes()),
      hidden: Some(true),
      ..Default::default()
    }),
    ..Default::default()
  };
  // NM cannot infer the security without an access point, so it is spelled out
  let key_mgmt = match security {
    HiddenSecurity::Open => None,
    HiddenSecurity::Wpa => Some(KEY_MGMT_PSK),
    HiddenSecurity::Wpa3 => Some(KEY_MGMT_SAE),
  };
  let wifi_security = key_mgmt.map(|key_mgmt| WifiSecurity {
    key_mgmt: Some(key_mgmt.to_owned()),
    psk: password,
    ..Default::default()
  });

  let mut settings = wifi.build();
  if let Some(wifi_security) = &wifi_security {
    settings.insert(WIFI_SECURITY_SETTING.to_owned(), wifi_security.build());
  }
  let nm = NetworkManager::new(conn).await?;
  nm.add_and_activate_connection(borrowed(&settings), &device, &ObjectPath::default())
    .await?;
  Ok(())
}

fn cert_path(path: &str) -> Result<Value<'static>> {
  anyhow::ensure!(
    Path::new(path).is_absolute(),
    "certificate path {path} is not absolute"
  );
  let mut uri = format!("file://{path}").into_bytes();
  uri.push(0);
  Ok(Value::from(uri))
}

fn string(value: impl Into<String>) -> Value<'static> {
  Value::from(value.into())
}

pub(crate) async fn connect_enterprise_wifi(
  conn: &Connection,
  device: OwnedObjectPath,
  config: EnterpriseConfig,
) -> Result<()> {
  let access_point = if config.hidden {
    OwnedObjectPath::default()
  } else {
    strongest_access_point(conn, &device, config.ssid.as_bytes()).await?
  };
  let settings = enterprise_settings(config)?;
  let nm = NetworkManager::new(conn).await?;
  nm.add_and_activate_connection(borrowed(&settings), &device, &access_point)
    .await?;
  Ok(())
}

fn enterprise_settings(config: EnterpriseConfig) -> Result<SettingsMap<'static>> {
  let eap = match config.eap {
    EapMethod::Peap => EAP_PEAP,
    EapMethod::Ttls => EAP_TTLS,
    EapMethod::Tls => EAP_TLS,
  };
  let mut enterprise = HashMap::from([
    (EAP_KEY.to_owned(), Value::from(vec![eap.to_owned()])),
    (IDENTITY_KEY.to_owned(), string(config.identity)),
  ]);
  let optional = [
    (ANONYMOUS_IDENTITY_KEY, config.anonymous_identity),
    (DOMAIN_KEY, config.domain),
    (PASSWORD_KEY, config.password),
    (PRIVATE_KEY_PASSWORD_KEY, config.private_key_password),
  ];
  for (key, value) in optional {
    if let Some(value) = value.filter(|value| !value.is_empty()) {
      enterprise.insert(key.to_owned(), string(value));
    }
  }
  if let Some(phase2) = config.phase2 {
    let phase2 = match phase2 {
      Phase2::Mschapv2 => PHASE2_MSCHAPV2,
      Phase2::Pap => PHASE2_PAP,
    };
    enterprise.insert(PHASE2_KEY.to_owned(), string(phase2));
  }
  match config.ca_cert.filter(|path| !path.is_empty()) {
    Some(path) => enterprise.insert(CA_CERT_KEY.to_owned(), cert_path(&path)?),
    None => enterprise.insert(SYSTEM_CA_KEY.to_owned(), Value::from(true)),
  };
  let certs = [
    (CLIENT_CERT_KEY, config.client_cert),
    (PRIVATE_KEY_KEY, config.private_key),
  ];
  for (key, path) in certs {
    if let Some(path) = path.filter(|path| !path.is_empty()) {
      enterprise.insert(key.to_owned(), cert_path(&path)?);
    }
  }

  let wifi = Settings {
    wifi: Some(WifiSettings {
      ssid: Some(config.ssid.into_bytes()),
      hidden: Some(config.hidden),
      ..Default::default()
    }),
    ..Default::default()
  };
  let wifi_security = WifiSecurity {
    key_mgmt: Some(KEY_MGMT_EAP.to_owned()),
    ..Default::default()
  };
  // the builders borrow their structs, owned copies outlive them
  let mut settings: SettingsMap<'static> = HashMap::new();
  let built = wifi
    .build()
    .into_iter()
    .chain([(WIFI_SECURITY_SETTING.to_owned(), wifi_security.build())]);
  for (name, setting) in built {
    let setting = setting
      .into_iter()
      .map(|(key, value)| Ok((key, value.try_to_owned()?.into())))
      .collect::<Result<_>>()?;
    settings.insert(name, setting);
  }
  settings.insert(ENTERPRISE_SETTING.to_owned(), enterprise);
  Ok(settings)
}

pub(crate) async fn connect_vpn(conn: &Connection, uuid: String) -> Result<()> {
  let settings = SettingsProxy::builder(conn)
    .cache_properties(CacheProperties::No)
    .build()
    .await?;
  let profile = settings.get_connection_by_uuid(&uuid).await?;
  // no device: NM brings the VPN up over the current default route
  let any = ObjectPath::default();
  let nm = NetworkManager::new(conn).await?;
  nm.activate_connection_by_paths(&profile, &any).await?;
  Ok(())
}

pub(crate) async fn disconnect_vpn(conn: &Connection, uuid: String) -> Result<()> {
  let nm = NetworkManager::new(conn).await?;
  for active in nm.active_connections().await? {
    if active.uuid().await.is_ok_and(|active| active == uuid) {
      nm.deactivate_connection(&active).await?;
      return Ok(());
    }
  }
  anyhow::bail!("{uuid} is not active")
}

pub(crate) async fn forget_wifi(
  conn: &Connection,
  device: OwnedObjectPath,
  ssid: Vec<u8>,
) -> Result<()> {
  let device = proxy::<DeviceProxy, Device>(conn, device).await?;
  for connection in saved_connections(&device, &ssid).await? {
    connection.delete().await?;
  }
  Ok(())
}

pub(crate) async fn connect_device(conn: &Connection, device: OwnedObjectPath) -> Result<()> {
  let any = ObjectPath::default();
  let nm = NetworkManager::new(conn).await?;
  nm.activate_connection_by_paths(&any, &device).await?;
  Ok(())
}

pub(crate) async fn disconnect_device(conn: &Connection, device: OwnedObjectPath) -> Result<()> {
  let device = proxy::<DeviceProxy, Device>(conn, device).await?;
  Ok(device.disconnect().await?)
}

/// deactivates the Corona Hotspot if it runs on `device`, unlike Device.Disconnect NM still
/// autoconnects it later; anything else on `device` stays up
pub(crate) async fn deactivate_hotspot_on(
  conn: &Connection,
  device: OwnedObjectPath,
) -> Result<()> {
  let device = proxy::<DeviceProxy, DeviceProxy>(conn, device).await?;
  let active = device.active_connection().await?;
  // nothing runs on it
  if active.as_str() == "/" {
    return Ok(());
  }
  let active = proxy::<ActiveConnectionProxy, ActiveConnection>(conn, active).await?;
  if active.id().await? != HOTSPOT_ID {
    return Ok(());
  }
  NetworkManager::new(conn)
    .await?
    .deactivate_connection(&active)
    .await?;
  Ok(())
}

/// the saved Corona Hotspot profile: its path and UUID
async fn saved_hotspot(conn: &Connection) -> Result<Option<(OwnedObjectPath, Option<String>)>> {
  let settings = SettingsProxy::builder(conn)
    .cache_properties(CacheProperties::No)
    .build()
    .await?;
  for path in settings.list_connections().await? {
    let Ok(saved) = profile(conn, path.clone()).await else {
      continue;
    };
    if let Some(connection) = saved
      .connection
      .filter(|c| c.id.as_deref() == Some(HOTSPOT_ID))
    {
      return Ok(Some((path, connection.uuid)));
    }
  }
  Ok(None)
}

/// the band and channel of a frequency in MHz, None outside 2.4 and 5 GHz
pub(crate) fn channel(frequency: u32) -> Option<(HotspotBand, u32)> {
  match frequency {
    2484 => Some((HotspotBand::Bg, 14)),
    2412..=2472 => Some((HotspotBand::Bg, (frequency - 2407) / 5)),
    5160..=5885 => Some((HotspotBand::A, (frequency - 5000) / 5)),
    _ => None,
  }
}

/// the band and channel a wifi device is connected on, None when it is not. Adding an interface
/// may restart wpa_supplicant (NixOS does), which drops `device` for a while: one `was_connected`
/// is waited for, like one still connecting
async fn client_channel(
  conn: &Connection,
  device: OwnedObjectPath,
  was_connected: bool,
  executor: &BackgroundExecutor,
) -> Result<Option<(HotspotBand, u32)>> {
  const ACTIVATED: u32 = 100;
  const FAILED: u32 = 120;
  let state = proxy::<DeviceProxy, DeviceProxy>(conn, device.clone()).await?;
  // a minute, eduroam's 802.1X takes its time
  for _ in 0..300 {
    match state.state().await? {
      ACTIVATED => break,
      // prepare up to secondaries
      40..=90 => {}
      // gave up, no reconnect is coming
      FAILED => return Ok(None),
      _ if was_connected => {}
      _ => return Ok(None),
    }
    executor.timer(Duration::from_millis(200)).await;
  }
  let access_point = proxy::<WirelessDeviceProxy, WirelessDeviceProxy>(conn, device)
    .await?
    .active_access_point()
    .await?;
  if access_point == OwnedObjectPath::default() {
    return Ok(None);
  }
  let frequency = proxy::<AccessPointProxy, AccessPointProxy>(conn, access_point)
    .await?
    .frequency()
    .await?;
  Ok(channel(frequency))
}

/// `channel` overrides the configured band
fn hotspot_settings(
  config: HotspotConfig,
  uuid: Option<String>,
  channel: Option<(HotspotBand, u32)>,
) -> SettingsMap<'static> {
  let mut connection = HashMap::from([
    ("id".to_owned(), string(HOTSPOT_ID)),
    ("type".to_owned(), string(WIFI_SETTING)),
    ("autoconnect".to_owned(), Value::from(false)),
  ]);
  let auto_off = config.auto_off.unwrap_or(0).to_string();
  if let Some(uuid) = uuid {
    connection.insert("uuid".to_owned(), string(uuid));
  }
  let band = channel.map_or(config.band, |(band, _)| band);
  let mut wifi = HashMap::from([
    ("ssid".to_owned(), Value::from(config.ssid.into_bytes())),
    ("mode".to_owned(), string(AP_MODE)),
    ("band".to_owned(), string(band.setting())),
    // a BSSID of its own each start, the radio's would follow the machine from place to place
    ("cloned-mac-address".to_owned(), string("random")),
    (
      "ap-isolation".to_owned(),
      Value::from(i32::from(config.isolate)),
    ),
  ]);
  if let Some((_, channel)) = channel {
    wifi.insert("channel".to_owned(), Value::from(channel));
  }
  let mut settings = HashMap::from([
    ("connection".to_owned(), connection),
    (WIFI_SETTING.to_owned(), wifi),
    // NM runs DHCP and NAT for the clients
    (
      "ipv4".to_owned(),
      HashMap::from([("method".to_owned(), string("shared"))]),
    ),
    // the firewall rules are IPv4 only, link-local IPv6 would reach the host past them
    (
      "ipv6".to_owned(),
      HashMap::from([("method".to_owned(), string("disabled"))]),
    ),
    (
      "user".to_owned(),
      HashMap::from([(
        "data".to_owned(),
        Value::from(HashMap::from([(AUTO_OFF_KEY.to_owned(), auto_off)])),
      )]),
    ),
  ]);
  if let Some(password) = config.password {
    let ccmp = || Value::from(vec!["ccmp".to_owned()]);
    let mut security = HashMap::from([
      ("key-mgmt".to_owned(), string(KEY_MGMT_PSK)),
      ("psk".to_owned(), string(password)),
      ("proto".to_owned(), Value::from(vec!["rsn".to_owned()])),
      ("pairwise".to_owned(), ccmp()),
      ("group".to_owned(), ccmp()),
      // against deauthentication by anyone around, optional for clients without
      ("pmf".to_owned(), Value::from(PMF_OPTIONAL)),
    ]);
    if config.wpa3 {
      // WPA3 requires it
      security.insert("key-mgmt".to_owned(), string(KEY_MGMT_SAE));
      security.insert("pmf".to_owned(), Value::from(PMF_REQUIRED));
    }
    settings.insert(WIFI_SECURITY_SETTING.to_owned(), security);
  }
  settings
}

/// `client`: the wifi device sharing `device`'s radio, which stays connected beside the hotspot,
/// and whether it was connected when asked
pub(crate) async fn start_hotspot(
  conn: &Connection,
  device: OwnedObjectPath,
  config: HotspotConfig,
  client: Option<(OwnedObjectPath, bool)>,
  executor: &BackgroundExecutor,
) -> Result<OwnedObjectPath> {
  config.validate()?;
  // the radio serves both on one channel, the client's
  let channel = match client {
    Some((client, was_connected)) => client_channel(conn, client, was_connected, executor).await?,
    None => None,
  };
  let nm = NetworkManager::new(conn).await?;
  let wpa3 = config.wpa3;
  let mut config = config;
  loop {
    // one profile, updated on every start, instead of a new one each time
    let active = match saved_hotspot(conn).await? {
      Some((path, uuid)) => {
        let settings = hotspot_settings(config.clone(), uuid, channel);
        proxy::<ConnectionSettingsProxy, ConnectionSettingsProxy>(conn, path.clone())
          .await?
          .update(borrowed(&settings))
          .await?;
        let active = nm.activate_connection_by_paths(&path, &device).await?;
        active.inner().path().to_owned().into()
      }
      None => {
        let settings = hotspot_settings(config.clone(), None, channel);
        nm.add_and_activate_connection(borrowed(&settings), &device, &ObjectPath::default())
          .await?
          .1
      }
    };
    if wait_for_activation(conn, active.clone(), executor).await? {
      return Ok(active);
    }
    // many radios run no WPA3 access point, the saved profile then shows WPA2
    if !(config.wpa3 && config.password.is_some()) {
      break;
    }
    tracing::warn!("the hotspot did not start with WPA3, falling back to WPA2");
    config.wpa3 = false;
  }
  // WPA2 failed too, WPA3 was not the cause: the profile keeps the choice
  if wpa3
    && !config.wpa3
    && let Some((path, uuid)) = saved_hotspot(conn).await?
  {
    config.wpa3 = true;
    let settings = hotspot_settings(config, uuid, channel);
    proxy::<ConnectionSettingsProxy, ConnectionSettingsProxy>(conn, path)
      .await?
      .update(borrowed(&settings))
      .await?;
  }
  match channel {
    // the band was the connection's, not the chosen one
    Some((_, channel)) => bail!(
      "the hotspot did not start on channel {channel}, which Wi-Fi is connected on, the wifi \
       device may not run an access point there"
    ),
    None => bail!("the hotspot did not start, the wifi device may not support the chosen band"),
  }
}

/// NM answers an activation once it is queued, a radio refusing the band fails it only later:
/// whether it came up
async fn wait_for_activation(
  conn: &Connection,
  active: OwnedObjectPath,
  executor: &BackgroundExecutor,
) -> Result<bool> {
  const ACTIVATED: u32 = 2;
  const DEACTIVATED: u32 = 4;
  let active = proxy::<ActiveConnectionProxy, ActiveConnectionProxy>(conn, active).await?;
  for _ in 0..150 {
    // NM drops a failed activation's object
    match active.state().await.ok() {
      Some(ACTIVATED) => return Ok(true),
      Some(DEACTIVATED) | None => break,
      _ => executor.timer(Duration::from_millis(200)).await,
    }
  }
  Ok(false)
}

/// asks for root through pkexec to add corona's access point interface on `beside`'s radio and let
/// its clients through the firewall
pub(crate) async fn add_ap_interface(beside: &str, locks: &[String]) -> Result<()> {
  let (phy, mac) = nl80211::radio(beside).with_context(|| format!("{beside} has no radio"))?;
  let mut args = vec![
    "add-ap-interface".to_owned(),
    phy.to_string(),
    nl80211::AP_INTERFACE.to_owned(),
    nl80211::format_mac(nl80211::ap_mac(mac)),
  ];
  args.extend(lock_args(locks));
  as_root(args).await
}

fn lock_args(locks: &[String]) -> impl Iterator<Item = String> + '_ {
  locks.iter().map(|lock| format!("--lock={lock}"))
}

/// keeps NM from connecting saved networks on corona's interface
pub(crate) async fn no_autoconnect(conn: &Connection, device: OwnedObjectPath) -> Result<()> {
  proxy::<DeviceProxy, DeviceProxy>(conn, device)
    .await?
    .set_autoconnect(false)
    .await?;
  Ok(())
}

/// asks for root through pkexec to let the clients of a hotspot on `interface` through the
/// firewall, which would drop their DHCP requests otherwise, closing `locks` to routing meanwhile
pub(crate) async fn allow_hotspot(interface: &str, locks: &[String]) -> Result<()> {
  let mut args = vec!["allow-hotspot".to_owned(), interface.to_owned()];
  args.extend(lock_args(locks));
  as_root(args).await
}

/// asks for root through pkexec to remove the firewall rules `allow_hotspot` added
pub(crate) async fn disallow_hotspot() -> Result<()> {
  as_root(vec!["disallow-hotspot".to_owned()]).await
}

/// runs `corona network <args>` through pkexec
async fn as_root(args: Vec<String>) -> Result<()> {
  // the tests run without pkexec
  if cfg!(test) {
    return Ok(());
  }
  let mut helper = Command::new("pkexec");
  helper
    .arg(std::env::current_exe()?)
    .arg("network")
    .args(args);
  // pkexec waits for the password, which corona's own polkit agent asks for on the UI thread
  let (done, output) = flume::bounded(1);
  std::thread::spawn(move || done.send(helper.output()));
  let output = output
    .recv_async()
    .await?
    .context("the hotspot needs pkexec")?;
  let stderr = String::from_utf8_lossy(&output.stderr);
  ensure!(output.status.success(), "{}", stderr.trim());
  // the helper's warnings, a firewall dropping the clients' DHCP say
  for line in stderr.lines().filter(|line| !line.trim().is_empty()) {
    tracing::warn!("corona network: {line}");
  }
  Ok(())
}

/// NM lists a new interface after a moment, and can use it once it left `unavailable`
pub(crate) async fn wait_for_device(
  conn: &Connection,
  name: &str,
  executor: &BackgroundExecutor,
) -> Result<OwnedObjectPath> {
  const DISCONNECTED: u32 = 30;
  let nm = NetworkManagerProxy::new(conn).await?;
  for _ in 0..50 {
    if let Ok(device) = nm.get_device_by_ip_iface(name).await {
      let state = proxy::<DeviceProxy, DeviceProxy>(conn, device.clone())
        .await?
        .state()
        .await?;
      if state >= DISCONNECTED {
        return Ok(device);
      }
    }
    executor.timer(Duration::from_millis(200)).await;
  }
  bail!("NetworkManager did not take up {name}")
}

/// the access point's associated stations, named by their DHCP leases: `expiry mac ip hostname
/// client-id` lines, `*` for no hostname
fn clients(stations: &[[u8; 6]], leases: &str) -> Vec<HotspotClient> {
  let leases: Vec<Vec<&str>> = leases
    .lines()
    .map(|line| line.split_whitespace().collect())
    .collect();
  stations
    .iter()
    .map(|&mac| {
      let lease = leases
        .iter()
        .find(|lease| lease.get(1).and_then(|m| nl80211::parse_mac(m)) == Some(mac));
      HotspotClient {
        mac: nl80211::format_mac(mac),
        ip: lease.and_then(|l| l.get(2)?.parse().ok()),
        hostname: lease
          .and_then(|l| l.get(3))
          .filter(|name| **name != "*")
          .map(|name| name.to_string()),
      }
    })
    .collect()
}

/// the devices connected to the hotspot on `interface`, from nl80211 and NM's dnsmasq
pub(crate) fn hotspot_clients(interface: &str) -> Result<Vec<HotspotClient>> {
  let stations = nl80211::stations(interface)?;
  // NM writes it world readable; absent until the first lease
  let leases = std::fs::read_to_string(format!(
    "/var/lib/NetworkManager/dnsmasq-{interface}.leases"
  ))
  .unwrap_or_default();
  Ok(clients(&stations, &leases))
}

pub(crate) async fn stop_hotspot(conn: &Connection, profile: OwnedObjectPath) -> Result<()> {
  let nm = NetworkManager::new(conn).await?;
  for active in nm.active_connections().await? {
    if active.connection_().await.is_ok_and(|c| c == profile) {
      nm.deactivate_connection(&active).await?;
      return Ok(());
    }
  }
  bail!("the hotspot is not running")
}

/// the running hotspot's profile, else the saved Corona Hotspot; None without either
pub(crate) async fn hotspot_config(
  conn: &Connection,
  running: Option<OwnedObjectPath>,
) -> Result<Option<HotspotConfig>> {
  let path = match running {
    Some(path) => path,
    None => match saved_hotspot(conn).await? {
      Some((path, _)) => path,
      None => return Ok(None),
    },
  };
  let profile = proxy::<ConnectionSettingsProxy, ConnectionSettingsProxy>(conn, path).await?;
  let mut settings = profile.get_settings().await?;
  let auto_off = auto_off(&settings);
  // NM's default, -1, is off
  let isolate = settings
    .get(WIFI_SETTING)
    .and_then(|wifi| wifi.get("ap-isolation"))
    .and_then(|isolation| i32::try_from(&**isolation).ok())
    == Some(1);
  let wifi = settings
    .remove(WIFI_SETTING)
    .map(WifiSettings::new)
    .unwrap_or_default();
  let wpa3 = settings
    .get(WIFI_SECURITY_SETTING)
    .and_then(|security| security.get("key-mgmt"))
    .and_then(|key_mgmt| <&str>::try_from(&**key_mgmt).ok())
    == Some(KEY_MGMT_SAE);
  // NM leaves the password out of the settings and hands it out on its own
  let password = if settings.contains_key(WIFI_SECURITY_SETTING) {
    let mut secrets = profile.get_secrets(WIFI_SECURITY_SETTING).await?;
    secrets
      .remove(WIFI_SECURITY_SETTING)
      .map(WifiSecurity::new)
      .and_then(|security| security.psk)
  } else {
    None
  };
  Ok(Some(HotspotConfig {
    ssid: String::from_utf8_lossy(&wifi.ssid.unwrap_or_default()).into_owned(),
    password,
    band: HotspotBand::from_setting(wifi.band.as_deref()),
    wpa3,
    auto_off,
    isolate,
  }))
}

#[cfg(test)]
mod tests {
  use std::net::Ipv4Addr;

  use super::*;

  fn config(eap: EapMethod) -> EnterpriseConfig {
    EnterpriseConfig {
      ssid: "eduroam".into(),
      hidden: false,
      eap,
      phase2: Some(Phase2::Pap),
      identity: "user@example.org".into(),
      anonymous_identity: Some(String::new()),
      password: None,
      ca_cert: None,
      domain: Some("example.org".into()),
      client_cert: None,
      private_key: None,
      private_key_password: None,
    }
  }

  #[test]
  fn enterprise_settings() {
    let settings = super::enterprise_settings(config(EapMethod::Ttls)).unwrap();
    let enterprise = &settings[ENTERPRISE_SETTING];
    assert_eq!(enterprise[EAP_KEY], Value::from(vec![EAP_TTLS.to_owned()]));
    assert_eq!(enterprise[PHASE2_KEY], string(PHASE2_PAP));
    assert_eq!(enterprise[SYSTEM_CA_KEY], Value::from(true));
    // empty and missing values are left for NM to ask for
    assert!(!enterprise.contains_key(ANONYMOUS_IDENTITY_KEY));
    assert!(!enterprise.contains_key(PASSWORD_KEY));
    assert_eq!(
      settings[WIFI_SECURITY_SETTING]["key-mgmt"],
      string(KEY_MGMT_EAP)
    );

    let tls = EnterpriseConfig {
      ca_cert: Some("/etc/ssl/ca.pem".into()),
      client_cert: Some("/home/user/cert.pem".into()),
      ..config(EapMethod::Tls)
    };
    let settings = super::enterprise_settings(tls).unwrap();
    let enterprise = &settings[ENTERPRISE_SETTING];
    assert_eq!(
      enterprise[CA_CERT_KEY],
      Value::from(b"file:///etc/ssl/ca.pem\0".to_vec())
    );
    assert!(!enterprise.contains_key(SYSTEM_CA_KEY));
    assert!(enterprise.contains_key(CLIENT_CERT_KEY));

    let relative = EnterpriseConfig {
      ca_cert: Some("ca.pem".into()),
      ..config(EapMethod::Peap)
    };
    assert!(super::enterprise_settings(relative).is_err());
  }

  #[test]
  fn enterprise_settings_in_full() {
    let peap = EnterpriseConfig {
      hidden: true,
      phase2: Some(Phase2::Mschapv2),
      anonymous_identity: Some("anonymous@example.org".into()),
      password: Some("pw".into()),
      ca_cert: Some(String::new()),
      private_key: Some(String::new()),
      ..config(EapMethod::Peap)
    };
    let settings = super::enterprise_settings(peap).unwrap();
    let enterprise = &settings[ENTERPRISE_SETTING];
    assert_eq!(enterprise[EAP_KEY], Value::from(vec![EAP_PEAP.to_owned()]));
    assert_eq!(enterprise[PHASE2_KEY], string(PHASE2_MSCHAPV2));
    assert_eq!(enterprise[IDENTITY_KEY], string("user@example.org"));
    assert_eq!(
      enterprise[ANONYMOUS_IDENTITY_KEY],
      string("anonymous@example.org")
    );
    assert_eq!(enterprise[PASSWORD_KEY], string("pw"));
    assert_eq!(enterprise[DOMAIN_KEY], string("example.org"));
    // an empty CA path means the system bundle, an empty key path means none
    assert_eq!(enterprise[SYSTEM_CA_KEY], Value::from(true));
    assert!(!enterprise.contains_key(PRIVATE_KEY_KEY));
    let wifi = &settings["802-11-wireless"];
    assert_eq!(wifi["ssid"], Value::from(b"eduroam".to_vec()));
    assert_eq!(wifi["hidden"], Value::from(true));

    let tls = EnterpriseConfig {
      phase2: None,
      private_key: Some("/home/user/key.pem".into()),
      private_key_password: Some("keypw".into()),
      ..config(EapMethod::Tls)
    };
    let settings = super::enterprise_settings(tls).unwrap();
    let enterprise = &settings[ENTERPRISE_SETTING];
    assert_eq!(enterprise[EAP_KEY], Value::from(vec![EAP_TLS.to_owned()]));
    assert!(!enterprise.contains_key(PHASE2_KEY));
    assert_eq!(
      enterprise[PRIVATE_KEY_KEY],
      Value::from(b"file:///home/user/key.pem\0".to_vec())
    );
    assert_eq!(enterprise[PRIVATE_KEY_PASSWORD_KEY], string("keypw"));

    for bad in [
      EnterpriseConfig {
        client_cert: Some("cert.pem".into()),
        ..config(EapMethod::Tls)
      },
      EnterpriseConfig {
        private_key: Some("./key.pem".into()),
        ..config(EapMethod::Tls)
      },
    ] {
      let error = super::enterprise_settings(bad).unwrap_err().to_string();
      assert!(error.ends_with("is not absolute"), "{error}");
    }
  }

  #[test]
  fn cert_paths() {
    assert_eq!(
      cert_path("/a b/ü.pem").unwrap(),
      Value::from("file:///a b/ü.pem\0".as_bytes().to_vec())
    );
    assert_eq!(
      cert_path("x").unwrap_err().to_string(),
      "certificate path x is not absolute"
    );
    assert!(cert_path("").is_err());
  }

  #[test]
  fn borrowed_settings_keep_every_value() {
    let settings: SettingsMap = HashMap::from([
      (
        "a".to_string(),
        HashMap::from([("k".to_string(), Value::from(1u32))]),
      ),
      ("b".to_string(), HashMap::new()),
    ]);
    let view = borrowed(&settings);
    assert_eq!(view.len(), 2);
    assert_eq!(view["a"]["k"], Value::from(1u32));
    assert!(view["b"].is_empty());
  }

  #[test]
  fn enterprise_config_from_json() {
    let config: EnterpriseConfig = serde_json::from_str(
      r#"{"ssid": "eduroam", "hidden": false, "eap": "ttls", "phase2": "pap", "identity": "me",
          "anonymous_identity": null, "password": null, "ca_cert": null, "domain": null,
          "client_cert": null, "private_key": null, "private_key_password": null}"#,
    )
    .unwrap();
    assert_eq!(
      (config.eap, config.phase2),
      (EapMethod::Ttls, Some(Phase2::Pap))
    );
    assert!(serde_json::from_str::<EnterpriseConfig>(r#"{"eap": "md5"}"#).is_err());
  }

  fn hotspot(ssid: &str, password: Option<&str>) -> HotspotConfig {
    HotspotConfig {
      ssid: ssid.into(),
      password: password.map(Into::into),
      band: HotspotBand::A,
      wpa3: false,
      auto_off: Some(10),
      isolate: true,
    }
  }

  #[test]
  fn hotspot_validation() {
    assert!(hotspot("corona", Some("12345678")).validate().is_ok());
    assert!(hotspot("corona", None).validate().is_ok());
    assert!(hotspot("corona", Some(&"a".repeat(63))).validate().is_ok());
    // a raw 64 hex digit key is fine, 64 other characters are not
    assert!(hotspot("corona", Some(&"ab".repeat(32))).validate().is_ok());
    assert!(hotspot("corona", Some(&"x".repeat(64))).validate().is_err());
    assert_eq!(
      hotspot("corona", Some("1234567"))
        .validate()
        .unwrap_err()
        .to_string(),
      "the hotspot password needs 8 to 63 characters"
    );
    assert!(hotspot("", None).validate().is_err());
    assert!(hotspot(&"s".repeat(33), None).validate().is_err());
  }

  #[test]
  fn hotspot_settings_spell_out_wpa2() {
    let settings = hotspot_settings(hotspot("corona", Some("password")), Some("u".into()), None);
    assert_eq!(settings["connection"]["id"], string(HOTSPOT_ID));
    assert_eq!(settings["connection"]["uuid"], string("u"));
    assert_eq!(settings["connection"]["autoconnect"], Value::from(false));
    assert_eq!(settings[WIFI_SETTING]["band"], string("a"));
    let security = &settings[WIFI_SECURITY_SETTING];
    assert_eq!(security["proto"], Value::from(vec!["rsn".to_owned()]));
    assert_eq!(security["group"], Value::from(vec!["ccmp".to_owned()]));

    let open = hotspot_settings(hotspot("corona", None), None, None);
    assert!(!open.contains_key(WIFI_SECURITY_SETTING));
    assert!(!open["connection"].contains_key("uuid"));
    assert!(!open[WIFI_SETTING].contains_key("channel"));

    // beside a connection on the same radio, its channel wins over the configured band
    let beside = hotspot_settings(hotspot("corona", None), None, Some((HotspotBand::Bg, 6)));
    assert_eq!(beside[WIFI_SETTING]["band"], string("bg"));
    assert_eq!(beside[WIFI_SETTING]["channel"], Value::from(6u32));
  }

  #[test]
  fn hotspot_clients_from_leases() {
    let leases = "1792496989 5e:0b:2a:11:42:07 10.42.0.65 Mac 01:5e:0b:2a:11:42:07\n\
                  1792496000 AA:BB:CC:DD:EE:FF 10.42.0.70 * *\n\
                  1792490000 12:34:56:78:9a:bc 10.42.0.80 gone *\n\
                  duid 00:01:00:01\n";
    let phone = [0x5e, 0x0b, 0x2a, 0x11, 0x42, 0x07];
    let unnamed = [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff];
    // associated, but no lease yet
    let fresh = [0x02, 0, 0, 0, 0, 0x01];
    assert_eq!(
      clients(&[phone, unnamed, fresh], leases),
      [
        HotspotClient {
          mac: "5e:0b:2a:11:42:07".into(),
          ip: Some(Ipv4Addr::new(10, 42, 0, 65)),
          hostname: Some("Mac".into()),
        },
        HotspotClient {
          mac: "aa:bb:cc:dd:ee:ff".into(),
          ip: Some(Ipv4Addr::new(10, 42, 0, 70)),
          hostname: None,
        },
        HotspotClient {
          mac: "02:00:00:00:00:01".into(),
          ip: None,
          hostname: None,
        },
      ]
    );
    // a lease outlives its client's connection
    assert_eq!(clients(&[], leases), []);
  }

  #[test]
  fn channels() {
    assert_eq!(channel(2412), Some((HotspotBand::Bg, 1)));
    assert_eq!(channel(2472), Some((HotspotBand::Bg, 13)));
    assert_eq!(channel(2484), Some((HotspotBand::Bg, 14)));
    assert_eq!(channel(5180), Some((HotspotBand::A, 36)));
    assert_eq!(channel(5825), Some((HotspotBand::A, 165)));
    // 6 GHz and nonsense
    assert_eq!(channel(5955), None);
    assert_eq!(channel(0), None);
  }

  #[test]
  fn hotspot_bands_round_trip() {
    for band in [HotspotBand::Bg, HotspotBand::A] {
      assert_eq!(HotspotBand::from_setting(Some(band.setting())), band);
    }
    // NM picks when unset, the hotspot is shown as 2.4 GHz then
    assert_eq!(HotspotBand::from_setting(None), HotspotBand::Bg);
  }

  #[test]
  fn eap_tls_missing_client_cert_and_private_key() {
    let tls_bare = config(EapMethod::Tls);
    let settings = super::enterprise_settings(tls_bare).unwrap();
    let enterprise = &settings[ENTERPRISE_SETTING];
    assert_eq!(enterprise[EAP_KEY], Value::from(vec![EAP_TLS.to_owned()]));
    assert!(!enterprise.contains_key(CLIENT_CERT_KEY));
    assert!(!enterprise.contains_key(PRIVATE_KEY_KEY));
  }

  #[test]
  fn generated_hotspots_are_valid() {
    let config = HotspotConfig::generated().unwrap();
    config.validate().unwrap();
    // tells nothing about the machine
    assert!(config.ssid.starts_with("Corona-") && config.ssid.len() == 11);
    let password = config.password.unwrap();
    assert_eq!(password.len(), 12);
    assert!(
      password
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() && !b"lo01".contains(&b))
    );
    assert_ne!(Some(password), HotspotConfig::generated().unwrap().password);
  }
}
