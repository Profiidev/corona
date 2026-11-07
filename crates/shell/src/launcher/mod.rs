//! Launcher rows and sources of the integrations

use gpui_kit::App;

mod bluetooth;
pub(crate) mod clipboard;
mod mpris;
mod notifications;
mod processes;
mod sysinfo;
mod tray;

pub(crate) fn register_all(cx: &mut App) {
  bluetooth::register(cx);
  clipboard::register(cx);
  mpris::register(cx);
  notifications::register(cx);
  sysinfo::register(cx);
  tray::register(cx);
  processes::register(cx);
}
