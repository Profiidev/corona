//! Settings sections of the integrations

use gpui_kit::App;

mod clipboard;
pub(crate) mod notifications;
mod printing;
mod sysinfo;

pub(crate) fn register_all(cx: &mut App) {
  sysinfo::register(cx);
  printing::register(cx);
  notifications::register(cx);
  clipboard::register(cx);
}
