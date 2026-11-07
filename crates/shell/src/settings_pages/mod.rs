//! Settings sections of the integrations

use gpui_kit::App;

mod clipboard;
pub(crate) mod notifications;
mod printing;
mod sysinfo;
mod tray;

pub(crate) fn register_all(cx: &mut App) {
  sysinfo::register(cx);
  tray::register(cx);
  printing::register(cx);
  notifications::register(cx);
  clipboard::register(cx);
}
