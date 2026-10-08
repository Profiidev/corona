use std::{collections::HashMap, path::Path};

use anyhow::{Context, Result, bail};
use cosmic_dbus_networkmanager::{
  device::{Device, wireless::WirelessDevice},
  interface::{
    device::{DeviceProxy, wireless::WirelessDeviceProxy},
    enums::NmConnectivityState,
    settings::SettingsProxy,
  },
  nm::NetworkManager,
  settings::connection::{Connection as NmConnection, Settings, WifiSecurity, WifiSettings},
};
use futures_lite::{StreamExt, future::or};
use serde::Deserialize;
use ts_rs::TS;
use zbus::{
  Connection,
  proxy::{Builder, CacheProperties, Defaults},
  zvariant::{ObjectPath, OwnedObjectPath, Value},
};

use crate::{
  agent::{
    ENTERPRISE_SETTING, IDENTITY_KEY, PASSWORD_KEY, PRIVATE_KEY_PASSWORD_KEY, WIFI_SECURITY_SETTING,
  },
  snapshot::{read_access_point, saved_ssid},
};

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
  ssid: &str,
) -> Result<OwnedObjectPath> {
  let wireless = proxy::<WirelessDeviceProxy, WirelessDevice>(conn, device.clone()).await?;
  let mut strongest = None;
  for ap in wireless.get_access_points().await? {
    let Ok(ap_info) = read_access_point(&ap).await else {
      continue;
    };
    if ap_info.ssid == ssid.as_bytes()
      && strongest
        .as_ref()
        .is_none_or(|(s, _)| ap_info.strength > *s)
    {
      strongest = Some((ap_info.strength, ap.inner().path().to_owned()));
    }
  }
  let (_, path) = strongest.with_context(|| format!("{ssid} is not in range"))?;
  Ok(path.into())
}

pub(crate) async fn connect_wifi(
  conn: &Connection,
  device: OwnedObjectPath,
  ssid: String,
) -> Result<()> {
  let nm = NetworkManager::new(conn).await?;
  let access_point = strongest_access_point(conn, &device, &ssid).await?;
  let device = proxy::<DeviceProxy, Device>(conn, device).await?;
  let saved = saved_connections(&device, ssid.as_bytes()).await?;
  if let Some(connection) = saved.first() {
    nm.activate_connection(connection, &device).await?;
    return Ok(());
  }

  let settings = Settings {
    wifi: Some(WifiSettings {
      ssid: Some(ssid.into_bytes()),
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
    strongest_access_point(conn, &device, &config.ssid).await?
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
  ssid: String,
) -> Result<()> {
  let device = proxy::<DeviceProxy, Device>(conn, device).await?;
  for connection in saved_connections(&device, ssid.as_bytes()).await? {
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

#[cfg(test)]
mod tests {
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
}
