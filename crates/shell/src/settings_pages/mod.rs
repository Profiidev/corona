//! Settings sections of the integrations

use gpui_kit::App;

mod brightness;
mod clipboard;
mod disks;
pub(crate) mod notifications;
pub(crate) mod pipewire;
mod printing;
mod sysinfo;
mod tray;

pub(crate) fn register_all(cx: &mut App) {
  sysinfo::register(cx);
  tray::register(cx);
  printing::register(cx);
  pipewire::register(cx);
  notifications::register(cx);
  clipboard::register(cx);
  disks::register(cx);
  brightness::register(cx);
}
