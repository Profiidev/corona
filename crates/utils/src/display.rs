use gpui_kit::{App, DisplayId};
use uuid::Uuid;

pub fn display_uuid(monitor: &str) -> Uuid {
  Uuid::new_v5(&Uuid::NAMESPACE_DNS, monitor.as_bytes())
}

pub fn display_id_for(monitor: &str, cx: &App) -> Option<DisplayId> {
  let uuid = display_uuid(monitor);
  cx.displays()
    .iter()
    .find(|d| d.uuid().ok() == Some(uuid))
    .map(|d| d.id())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn uuids_are_stable_per_name() {
    assert_eq!(display_uuid("DP-1"), display_uuid("DP-1"));
    assert_ne!(display_uuid("DP-1"), display_uuid("DP-2"));
    assert_eq!(display_uuid("DP-1").get_version_num(), 5);
    // pinned: displays are matched across processes by this value
    assert_eq!(
      display_uuid("").to_string(),
      Uuid::new_v5(&Uuid::NAMESPACE_DNS, b"").to_string()
    );
    assert_ne!(display_uuid("ü"), display_uuid("u"));
  }
}
