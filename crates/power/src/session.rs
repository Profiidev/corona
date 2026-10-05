use std::env;

use anyhow::{Context, Result};
use zbus::{Connection, proxy::CacheProperties};

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

/// `nixos-generation-848-3f7m….efi` as "NixOS generation 848", `auto-windows` as
/// "Windows", other ids without their extension
pub fn entry_title(entry: &str) -> String {
  let id = entry.trim_end_matches(".conf").trim_end_matches(".efi");
  if let Some(rest) = id.strip_prefix("nixos-generation-") {
    let generation = rest.split('-').next().unwrap_or(rest);
    return format!("NixOS generation {generation}");
  }
  let id = id.strip_prefix("auto-").unwrap_or(id);
  let mut title = id.replace(['-', '_'], " ");
  if let Some(first) = title.get_mut(..1) {
    first.make_ascii_uppercase();
  }
  title
}

#[cfg(test)]
mod tests {
  use super::entry_title;

  #[test]
  fn titles() {
    assert_eq!(
      entry_title("nixos-generation-848-3f7mucq7oo66hfrwx4646wg5ugctdvn3hevnd5yzqvej7mpb5zta.efi"),
      "NixOS generation 848"
    );
    assert_eq!(
      entry_title("nixos-generation-169.conf"),
      "NixOS generation 169"
    );
    assert_eq!(entry_title("auto-windows"), "Windows");
    assert_eq!(entry_title("rescue.efi"), "Rescue");
  }
}
