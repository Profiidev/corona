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
  /// what NM asked for, a cancel names the same
  pub(crate) connection: OwnedObjectPath,
  pub(crate) setting: String,
}

pub(crate) enum AgentEvent {
  Request(SecretRequest),
  Cancel {
    connection: OwnedObjectPath,
    setting: String,
  },
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
    connection_path: OwnedObjectPath,
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
      connection: connection_path,
      setting: setting_name.clone(),
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

  async fn cancel_get_secrets(&self, connection_path: OwnedObjectPath, setting_name: String) {
    let cancel = AgentEvent::Cancel {
      connection: connection_path,
      setting: setting_name,
    };
    let _ = self.events.send_async(cancel).await;
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

#[cfg(test)]
mod tests {
  use futures_lite::future::block_on;
  use zbus::zvariant::Value;

  use super::*;

  fn settings(pairs: &[(&str, &str, Value<'_>)]) -> SettingsMap {
    let mut map = SettingsMap::new();
    for (setting, key, value) in pairs {
      map
        .entry(setting.to_string())
        .or_default()
        .insert(key.to_string(), value.try_to_owned().unwrap());
    }
    map
  }

  fn wifi(ssid: &[u8]) -> SettingsMap {
    settings(&[
      ("connection", "id", "Home profile".into()),
      ("802-11-wireless", "ssid", ssid.to_vec().into()),
    ])
  }

  /// asks the agent and answers with `answer` once the request arrives
  fn ask(
    connection: SettingsMap,
    setting: &str,
    hints: Vec<String>,
    flags: u32,
    answer: impl FnOnce(SecretRequest) + Send + 'static,
  ) -> Result<SettingsMap, AgentError> {
    let (events, received) = flume::unbounded();
    let agent = SecretAgent { events };
    let answering = std::thread::spawn(move || {
      if let Ok(AgentEvent::Request(request)) = received.recv() {
        answer(request);
      }
    });
    let result = block_on(agent.get_secrets(
      connection,
      OwnedObjectPath::try_from("/org/freedesktop/NetworkManager/Settings/2").unwrap(),
      setting.into(),
      hints,
      flags,
    ));
    drop(agent);
    answering.join().unwrap();
    result
  }

  fn secret(password: &str, identity: Option<&str>) -> Secret {
    Secret {
      password: password.into(),
      identity: identity.map(Into::into),
    }
  }

  fn value(result: &SettingsMap, setting: &str, key: &str) -> Option<String> {
    String::try_from(result.get(setting)?.get(key)?.try_clone().unwrap()).ok()
  }

  #[test]
  fn wifi_passwords() {
    let result = ask(
      wifi(b"home"),
      WIFI_SECURITY_SETTING,
      vec![],
      FLAG_ALLOW_INTERACTION,
      |request| {
        assert_eq!(
          (request.name.as_str(), request.kind),
          ("home", SecretKind::Psk)
        );
        assert!(!request.retry && request.identity.is_none());
        request
          .reply
          .send(secret("hunter22", Some("ignored")))
          .unwrap();
      },
    )
    .unwrap();
    assert_eq!(
      value(&result, WIFI_SECURITY_SETTING, PSK_KEY).as_deref(),
      Some("hunter22")
    );
    // a psk never sends an identity
    assert_eq!(result[WIFI_SECURITY_SETTING].len(), 1);
  }

  #[test]
  fn retries_and_odd_names() {
    ask(
      wifi(b"caf\xe9"),
      WIFI_SECURITY_SETTING,
      vec![],
      FLAG_ALLOW_INTERACTION | FLAG_REQUEST_NEW,
      |request| {
        assert!(request.retry);
        assert_eq!(request.name, "caf\u{fffd}");
      },
    )
    .unwrap_err();
    // wired 802.1X: named by the profile
    let wired = settings(&[("connection", "id", "Office LAN".into())]);
    ask(
      wired,
      ENTERPRISE_SETTING,
      vec![],
      FLAG_ALLOW_INTERACTION,
      |request| {
        assert_eq!(request.name, "Office LAN");
      },
    )
    .unwrap_err();
    ask(
      SettingsMap::new(),
      ENTERPRISE_SETTING,
      vec![],
      FLAG_ALLOW_INTERACTION,
      |request| {
        assert_eq!(request.name, "");
      },
    )
    .unwrap_err();
  }

  #[test]
  fn enterprise_logins() {
    let mut eduroam = wifi(b"eduroam");
    eduroam.extend(settings(&[(
      ENTERPRISE_SETTING,
      IDENTITY_KEY,
      "me@uni.example".into(),
    )]));
    let result = ask(
      eduroam,
      ENTERPRISE_SETTING,
      vec![],
      FLAG_ALLOW_INTERACTION,
      |request| {
        assert_eq!(request.kind, SecretKind::Enterprise);
        assert_eq!(request.identity.as_deref(), Some("me@uni.example"));
        request
          .reply
          .send(secret("pw", Some("other@uni.example")))
          .unwrap();
      },
    )
    .unwrap();
    assert_eq!(
      value(&result, ENTERPRISE_SETTING, PASSWORD_KEY).as_deref(),
      Some("pw")
    );
    assert_eq!(
      value(&result, ENTERPRISE_SETTING, IDENTITY_KEY).as_deref(),
      Some("other@uni.example")
    );

    // an empty stored identity counts as none, a missing answer keeps it out
    let mut blank = wifi(b"eduroam");
    blank.extend(settings(&[(ENTERPRISE_SETTING, IDENTITY_KEY, "".into())]));
    let result = ask(
      blank,
      ENTERPRISE_SETTING,
      vec![],
      FLAG_ALLOW_INTERACTION,
      |request| {
        assert_eq!(request.identity, None);
        request.reply.send(secret("pw", None)).unwrap();
      },
    )
    .unwrap();
    assert_eq!(value(&result, ENTERPRISE_SETTING, IDENTITY_KEY), None);

    // EAP-TLS asks for the key's password
    let result = ask(
      wifi(b"eduroam"),
      ENTERPRISE_SETTING,
      vec!["other".into(), PRIVATE_KEY_PASSWORD_KEY.into()],
      FLAG_ALLOW_INTERACTION,
      |request| request.reply.send(secret("keypw", None)).unwrap(),
    )
    .unwrap();
    assert_eq!(
      value(&result, ENTERPRISE_SETTING, PRIVATE_KEY_PASSWORD_KEY).as_deref(),
      Some("keypw")
    );
    assert_eq!(value(&result, ENTERPRISE_SETTING, PASSWORD_KEY), None);
  }

  #[test]
  fn refusals() {
    let error = ask(wifi(b"x"), "vpn", vec![], FLAG_ALLOW_INTERACTION, |_| {
      panic!("not asked")
    })
    .unwrap_err();
    assert!(matches!(error, AgentError::NoSecrets(ref m) if m == "vpn is not supported"));
    let error = ask(wifi(b"x"), WIFI_SECURITY_SETTING, vec![], 0, |_| {
      panic!("not asked")
    })
    .unwrap_err();
    assert!(matches!(error, AgentError::NoSecrets(ref m) if m == "interaction not allowed"));
    // the user closing the prompt
    let error = ask(
      wifi(b"x"),
      WIFI_SECURITY_SETTING,
      vec![],
      FLAG_ALLOW_INTERACTION,
      drop,
    )
    .unwrap_err();
    assert!(matches!(error, AgentError::UserCanceled(ref m) if m == "canceled"));
  }

  #[test]
  fn without_a_shell() {
    let (events, received) = flume::unbounded();
    drop(received);
    let agent = SecretAgent { events };
    let error = block_on(agent.get_secrets(
      wifi(b"x"),
      OwnedObjectPath::default(),
      WIFI_SECURITY_SETTING.into(),
      vec![],
      FLAG_ALLOW_INTERACTION,
    ))
    .unwrap_err();
    assert!(matches!(error, AgentError::UserCanceled(ref m) if m == "shell is gone"));
  }

  #[test]
  fn cancel_and_storage() {
    let (events, received) = flume::unbounded();
    let agent = SecretAgent { events };
    block_on(agent.cancel_get_secrets(OwnedObjectPath::default(), WIFI_SECURITY_SETTING.into()));
    assert!(matches!(
      received.try_recv(),
      Ok(AgentEvent::Cancel { setting, .. }) if setting == WIFI_SECURITY_SETTING
    ));
    // nothing is stored by the agent
    block_on(agent.save_secrets(SettingsMap::new(), OwnedObjectPath::default()));
    block_on(agent.delete_secrets(SettingsMap::new(), OwnedObjectPath::default()));
    assert!(received.is_empty());
  }
}
