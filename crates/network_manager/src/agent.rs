use std::collections::HashMap;

use anyhow::Result;
use cosmic_dbus_networkmanager::settings::connection::Settings;
use nm_secret_agent_manager::AgentManagerProxy;
use zbus::{
  Connection,
  zvariant::{OwnedObjectPath, OwnedValue, Str},
};

const AGENT_PATH: &str = "/org/freedesktop/NetworkManager/SecretAgent";
const AGENT_ID: &str = "io.corona.shell";
pub(crate) const WIFI_SECURITY_SETTING: &str = "802-11-wireless-security";
const PSK_KEY: &str = "psk";
pub(crate) const ENTERPRISE_SETTING: &str = "802-1x";
pub(crate) const IDENTITY_KEY: &str = "identity";
pub(crate) const PASSWORD_KEY: &str = "password";
// EAP-TLS asks for the private key's password instead of a login password
pub(crate) const PRIVATE_KEY_PASSWORD_KEY: &str = "private-key-password";
const FLAG_ALLOW_INTERACTION: u32 = 0x1;
const FLAG_REQUEST_NEW: u32 = 0x2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecretKind {
  /// WPA/WPA2/WPA3 personal
  Psk,
  /// 802.1X, like eduroam: a login password, with a username when the profile has none
  Enterprise,
}

pub struct Secret {
  pub password: String,
  /// only read for `SecretKind::Enterprise`, None keeps the profile's
  pub identity: Option<String>,
}

/// NetworkManager wants a secret, answer it with `NetworkManager::answer_secret`
pub struct SecretRequest {
  /// the SSID, or the profile name for a wired 802.1X connection
  pub name: String,
  pub kind: SecretKind,
  /// the username stored in an 802.1X profile, None when it has to be asked for
  pub identity: Option<String>,
  /// the previous secret was rejected
  pub retry: bool,
  pub(crate) reply: flume::Sender<Secret>,
}

pub(crate) enum AgentEvent {
  Request(SecretRequest),
  Cancel,
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.freedesktop.NetworkManager.SecretAgent")]
enum AgentError {
  #[zbus(error)]
  ZBus(zbus::Error),
  NoSecrets(String),
  UserCanceled(String),
}

struct SecretAgent {
  events: flume::Sender<AgentEvent>,
}

type SettingsMap = HashMap<String, HashMap<String, OwnedValue>>;

#[zbus::interface(name = "org.freedesktop.NetworkManager.SecretAgent")]
impl SecretAgent {
  async fn get_secrets(
    &self,
    connection: SettingsMap,
    _connection_path: OwnedObjectPath,
    setting_name: String,
    hints: Vec<String>,
    flags: u32,
  ) -> Result<SettingsMap, AgentError> {
    let kind = match setting_name.as_str() {
      WIFI_SECURITY_SETTING => SecretKind::Psk,
      ENTERPRISE_SETTING => SecretKind::Enterprise,
      _ => {
        return Err(AgentError::NoSecrets(format!(
          "{setting_name} is not supported"
        )));
      }
    };
    if flags & FLAG_ALLOW_INTERACTION == 0 {
      return Err(AgentError::NoSecrets("interaction not allowed".into()));
    }

    let identity = connection
      .get(ENTERPRISE_SETTING)
      .and_then(|setting| setting.get(IDENTITY_KEY))
      .and_then(|identity| String::try_from(identity.clone()).ok())
      .filter(|identity| !identity.is_empty());
    let settings = Settings::new(connection);
    let name = settings
      .wifi
      .and_then(|wifi| wifi.ssid)
      .map(|ssid| String::from_utf8_lossy(&ssid).into_owned())
      .or_else(|| settings.connection.and_then(|connection| connection.id))
      .unwrap_or_default();

    let (reply, answer) = flume::bounded(1);
    let request = SecretRequest {
      name,
      kind,
      identity,
      retry: flags & FLAG_REQUEST_NEW != 0,
      reply,
    };
    self
      .events
      .send_async(AgentEvent::Request(request))
      .await
      .map_err(|_| AgentError::UserCanceled("shell is gone".into()))?;

    // the UI drops the request on cancel, closing the channel
    let secret = answer
      .recv_async()
      .await
      .map_err(|_| AgentError::UserCanceled("canceled".into()))?;

    let password_key = match kind {
      SecretKind::Psk => PSK_KEY,
      _ if hints.iter().any(|hint| hint == PRIVATE_KEY_PASSWORD_KEY) => PRIVATE_KEY_PASSWORD_KEY,
      SecretKind::Enterprise => PASSWORD_KEY,
    };
    let mut secrets = HashMap::from([(password_key.to_owned(), owned(secret.password))]);
    if let (SecretKind::Enterprise, Some(identity)) = (kind, secret.identity) {
      secrets.insert(IDENTITY_KEY.to_owned(), owned(identity));
    }
    Ok(HashMap::from([(setting_name, secrets)]))
  }

  async fn cancel_get_secrets(&self, _connection_path: OwnedObjectPath, _setting_name: String) {
    let _ = self.events.send_async(AgentEvent::Cancel).await;
  }

  // NM stores the returned secrets itself, nothing is agent owned
  async fn save_secrets(&self, _connection: SettingsMap, _connection_path: OwnedObjectPath) {}

  async fn delete_secrets(&self, _connection: SettingsMap, _connection_path: OwnedObjectPath) {}
}

fn owned(value: String) -> OwnedValue {
  OwnedValue::from(Str::from(value))
}

pub(crate) async fn register(conn: &Connection) -> Result<flume::Receiver<AgentEvent>> {
  let (events, rx) = flume::unbounded();
  conn
    .object_server()
    .at(AGENT_PATH, SecretAgent { events })
    .await?;
  AgentManagerProxy::new(conn)
    .await?
    .register(AGENT_ID)
    .await?;
  Ok(rx)
}
