//! The IPC commands of the integrations

use gpui_kit::App;

pub mod bluetooth;
pub mod mpris;
pub mod notifications;
pub mod sysinfo;

pub(crate) fn register_all(cx: &mut App) {
  bluetooth::register(cx);
  mpris::register(cx);
  notifications::register(cx);
  sysinfo::register(cx);
}
