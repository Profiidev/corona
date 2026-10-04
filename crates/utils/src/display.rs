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
