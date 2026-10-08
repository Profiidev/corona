use std::env;

use anyhow::{Context, Result};
use futures_lite::StreamExt;
use gpui_kit::{AsyncApp, Task};
use zbus::{Connection, proxy::CacheProperties, zvariant::OwnedObjectPath};

#[zbus::proxy(
  interface = "org.freedesktop.login1.Manager",
  default_service = "org.freedesktop.login1",
  default_path = "/org/freedesktop/login1"
)]
pub(crate) trait LoginManager {
  fn power_off(&self, interactive: bool) -> zbus::Result<()>;
  fn reboot(&self, interactive: bool) -> zbus::Result<()>;
  fn suspend(&self, interactive: bool) -> zbus::Result<()>;
  fn hibernate(&self, interactive: bool) -> zbus::Result<()>;
  fn suspend_then_hibernate(&self, interactive: bool) -> zbus::Result<()>;

  /// `yes`, `no`, `challenge` (needs authentication) or `na`
  fn can_power_off(&self) -> zbus::Result<String>;
  fn can_reboot(&self) -> zbus::Result<String>;
  fn can_suspend(&self) -> zbus::Result<String>;
  fn can_hibernate(&self) -> zbus::Result<String>;
  fn can_suspend_then_hibernate(&self) -> zbus::Result<String>;
  fn can_reboot_to_firmware_setup(&self) -> zbus::Result<String>;
  fn can_reboot_to_boot_loader_entry(&self) -> zbus::Result<String>;

  fn set_reboot_to_firmware_setup(&self, enable: bool) -> zbus::Result<()>;
  fn set_reboot_to_boot_loader_entry(&self, entry: &str) -> zbus::Result<()>;

  /// the boot loader's entry ids, like `nixos-generation-848.conf` or `auto-windows`
  #[zbus(property)]
  fn boot_loader_entries(&self) -> zbus::Result<Vec<String>>;

  fn terminate_session(&self, session: &str) -> zbus::Result<()>;

  fn get_session(&self, session: &str) -> zbus::Result<OwnedObjectPath>;

  /// Held off until the returned fd closes. A `delay` lock lasts at most
  /// `InhibitDelayMaxSec`, 5s by default.
  fn inhibit(
    &self,
    what: &str,
    who: &str,
    why: &str,
    mode: &str,
  ) -> zbus::Result<zbus::zvariant::OwnedFd>;

  /// `true` right before suspend or hibernate, `false` after resume
  #[zbus(signal)]
  fn prepare_for_sleep(&self, start: bool) -> zbus::Result<()>;
}

#[zbus::proxy(
  interface = "org.freedesktop.login1.Session",
  default_service = "org.freedesktop.login1"
)]
pub(crate) trait LoginSession {
  /// `loginctl lock-session`
  #[zbus(signal)]
  fn lock(&self) -> zbus::Result<()>;

  /// `loginctl unlock-session`
  #[zbus(signal)]
  fn unlock(&self) -> zbus::Result<()>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionAction {
  Logout,
  Suspend,
  Hibernate,
  SuspendThenHibernate,
  Reboot,
  PowerOff,
  RebootToFirmware,
}

/// What this machine allows, read when the session menu opens
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionCapabilities {
  pub suspend: bool,
  pub hibernate: bool,
  pub suspend_then_hibernate: bool,
  pub reboot: bool,
  pub power_off: bool,
  pub reboot_to_firmware: bool,
  /// empty when the boot loader can't be told the next entry
  pub boot_entries: Vec<String>,
}

async fn manager(conn: &Connection) -> Result<LoginManagerProxy<'_>> {
  Ok(
    LoginManagerProxy::builder(conn)
      .cache_properties(CacheProperties::No)
      .build()
      .await?,
  )
}

fn session_id() -> Result<String> {
  env::var("XDG_SESSION_ID").context("XDG_SESSION_ID is not set")
}

/// `challenge` counts too, polkit asks for the password then
fn allowed(answer: zbus::Result<String>) -> bool {
  answer.is_ok_and(|a| a == "yes" || a == "challenge")
}

pub(crate) async fn capabilities(conn: Connection) -> Result<SessionCapabilities> {
  let manager = manager(&conn).await?;
  let boot_entries = match allowed(manager.can_reboot_to_boot_loader_entry().await) {
    true => manager.boot_loader_entries().await.unwrap_or_default(),
    false => Vec::new(),
  };
  let suspend = allowed(manager.can_suspend().await);
  let hibernate = allowed(manager.can_hibernate().await);
  Ok(SessionCapabilities {
    suspend,
    hibernate,
    // logind answers for its own target, which works only when both of its steps do
    suspend_then_hibernate: suspend
      && hibernate
      && allowed(manager.can_suspend_then_hibernate().await),
    reboot: allowed(manager.can_reboot().await),
    power_off: allowed(manager.can_power_off().await),
    reboot_to_firmware: allowed(manager.can_reboot_to_firmware_setup().await),
    boot_entries,
  })
}

pub(crate) async fn run(conn: Connection, action: SessionAction) -> Result<()> {
  let manager = manager(&conn).await?;
  match action {
    SessionAction::Logout => manager.terminate_session(&session_id()?).await?,
    SessionAction::Suspend => manager.suspend(true).await?,
    SessionAction::Hibernate => manager.hibernate(true).await?,
    SessionAction::SuspendThenHibernate => manager.suspend_then_hibernate(true).await?,
    SessionAction::Reboot => manager.reboot(true).await?,
    SessionAction::PowerOff => manager.power_off(true).await?,
    SessionAction::RebootToFirmware => {
      manager.set_reboot_to_firmware_setup(true).await?;
      manager.reboot(true).await?
    }
  }
  Ok(())
}

pub(crate) async fn reboot_to(conn: Connection, entry: String) -> Result<()> {
  let manager = manager(&conn).await?;
  manager.set_reboot_to_boot_loader_entry(&entry).await?;
  Ok(manager.reboot(true).await?)
}

/// Holds every suspend and hibernate back until `before_sleep`'s task ends.
pub(crate) async fn before_sleep(
  conn: Connection,
  cx: &mut AsyncApp,
  before_sleep: impl Fn(&mut gpui_kit::App) -> Task<()>,
) -> Result<()> {
  let manager = manager(&conn).await?;
  let mut signals = manager.receive_prepare_for_sleep().await?;
  let mut next = async |start| loop {
    let signal = signals.next().await.context("logind went away")?;
    if signal.args()?.start == start {
      return anyhow::Ok(());
    }
  };
  loop {
    let inhibitor = manager
      .inhibit("sleep", "corona", "Lock the screen", "delay")
      .await?;
    next(true).await?;
    cx.update(&before_sleep).await;
    drop(inhibitor);
    next(false).await?;
  }
}

/// Calls `on` with `true` when logind asks this session to lock, `false` to unlock
pub(crate) async fn lock_requests(
  conn: Connection,
  cx: &mut AsyncApp,
  on: impl Fn(bool, &mut gpui_kit::App),
) -> Result<()> {
  let path = manager(&conn).await?.get_session(&session_id()?).await?;
  let session = LoginSessionProxy::builder(&conn)
    .path(path)?
    .cache_properties(CacheProperties::No)
    .build()
    .await?;
  let locks = session.receive_lock().await?.map(|_| true);
  let unlocks = session.receive_unlock().await?.map(|_| false);
  let mut requests = locks.or(unlocks);
  while let Some(lock) = requests.next().await {
    cx.update(|cx| on(lock, cx));
  }
  anyhow::bail!("logind went away")
}

/// What a boot entry is called, for the shell to word
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryTitle {
  /// `nixos-generation-848-3f7m….efi` as generation `848`
  NixosGeneration(String),
  /// `auto-windows` as "Windows", other ids without their extension
  Other(String),
}

pub fn entry_title(entry: &str) -> EntryTitle {
  let id = entry.trim_end_matches(".conf").trim_end_matches(".efi");
  if let Some(rest) = id.strip_prefix("nixos-generation-") {
    let generation = rest.split('-').next().unwrap_or(rest);
    return EntryTitle::NixosGeneration(generation.to_string());
  }
  let id = id.strip_prefix("auto-").unwrap_or(id);
  let mut title = id.replace(['-', '_'], " ");
  if let Some(first) = title.get_mut(..1) {
    first.make_ascii_uppercase();
  }
  EntryTitle::Other(title)
}

#[cfg(test)]
mod tests {
  use super::{EntryTitle, entry_title};

  #[test]
  fn titles() {
    assert_eq!(
      entry_title("nixos-generation-848-3f7mucq7oo66hfrwx4646wg5ugctdvn3hevnd5yzqvej7mpb5zta.efi"),
      EntryTitle::NixosGeneration("848".into())
    );
    assert_eq!(
      entry_title("nixos-generation-169.conf"),
      EntryTitle::NixosGeneration("169".into())
    );
    assert_eq!(
      entry_title("auto-windows"),
      EntryTitle::Other("Windows".into())
    );
    assert_eq!(
      entry_title("rescue.efi"),
      EntryTitle::Other("Rescue".into())
    );
  }

  #[test]
  fn title_edges() {
    assert_eq!(
      entry_title("nixos-generation-"),
      EntryTitle::NixosGeneration("".into())
    );
    assert_eq!(
      entry_title("nixos-generation-12"),
      EntryTitle::NixosGeneration("12".into())
    );
    assert_eq!(entry_title(""), EntryTitle::Other("".into()));
    assert_eq!(
      entry_title("arch_linux-lts.conf"),
      EntryTitle::Other("Arch linux lts".into())
    );
    assert_eq!(
      entry_title("auto-reboot-to-firmware-setup"),
      EntryTitle::Other("Reboot to firmware setup".into())
    );
    // a first letter that is not ASCII stays as it is
    assert_eq!(entry_title("über.conf"), EntryTitle::Other("über".into()));
    assert_eq!(entry_title("x.conf.conf"), EntryTitle::Other("X".into()));
  }

  #[test]
  fn allowed_answers() {
    assert!(super::allowed(Ok("yes".into())));
    assert!(super::allowed(Ok("challenge".into())));
    for no in ["no", "na", "", "YES"] {
      assert!(!super::allowed(Ok(no.into())), "{no}");
    }
    assert!(!super::allowed(Err(zbus::Error::InvalidReply)));
  }
}
