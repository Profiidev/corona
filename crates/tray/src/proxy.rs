use std::collections::HashMap;

use zbus::{proxy, zvariant::OwnedValue};

pub(crate) const WATCHER_NAME: &str = "org.kde.StatusNotifierWatcher";
pub(crate) const WATCHER_PATH: &str = "/StatusNotifierWatcher";
pub(crate) const ITEM_PATH: &str = "/StatusNotifierItem";
pub(crate) const ITEM_INTERFACE: &str = "org.kde.StatusNotifierItem";
pub(crate) const MENU_INTERFACE: &str = "com.canonical.dbusmenu";

#[proxy(
  interface = "org.kde.StatusNotifierWatcher",
  default_service = "org.kde.StatusNotifierWatcher",
  default_path = "/StatusNotifierWatcher"
)]
pub(crate) trait Watcher {
  fn register_status_notifier_host(&self, service: &str) -> zbus::Result<()>;

  #[zbus(property)]
  fn registered_status_notifier_items(&self) -> zbus::Result<Vec<String>>;
}

#[proxy(interface = "org.kde.StatusNotifierItem", assume_defaults = true)]
pub(crate) trait Item {
  fn activate(&self, x: i32, y: i32) -> zbus::Result<()>;

  fn secondary_activate(&self, x: i32, y: i32) -> zbus::Result<()>;

  fn context_menu(&self, x: i32, y: i32) -> zbus::Result<()>;

  fn scroll(&self, delta: i32, orientation: &str) -> zbus::Result<()>;
}

pub(crate) type Layout = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);

#[proxy(interface = "com.canonical.dbusmenu", assume_defaults = true)]
pub(crate) trait Menu {
  fn about_to_show(&self, id: i32) -> zbus::Result<bool>;

  fn event(
    &self,
    id: i32,
    event_id: &str,
    data: &zbus::zvariant::Value<'_>,
    timestamp: u32,
  ) -> zbus::Result<()>;

  fn get_layout(
    &self,
    parent_id: i32,
    recursion_depth: i32,
    property_names: &[&str],
  ) -> zbus::Result<(u32, Layout)>;
}

pub(crate) fn parse_address(address: &str) -> (&str, String) {
  match address.split_once('/') {
    Some((bus, path)) => (bus, format!("/{path}")),
    None => (address, ITEM_PATH.to_string()),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn address() {
    assert_eq!(
      parse_address(":1.42/org/ayatana/NotificationItem/x"),
      (":1.42", "/org/ayatana/NotificationItem/x".to_string())
    );
    assert_eq!(
      parse_address("org.kde.StatusNotifierItem-1-1"),
      ("org.kde.StatusNotifierItem-1-1", ITEM_PATH.to_string())
    );
  }
}
