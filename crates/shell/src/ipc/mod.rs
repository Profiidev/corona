//! The IPC commands of the integrations

use gpui_kit::App;

pub mod bluetooth;
pub mod disks;
pub mod idle;
pub mod brightness;
pub mod pipewire;
pub mod power;
pub mod clipboard;
pub mod mpris;
pub mod network;
pub mod notifications;
pub mod sysinfo;
pub mod tray;

pub(crate) fn register_all(cx: &mut App) {
  bluetooth::register(cx);
  disks::register(cx);
  idle::register(cx);
  brightness::register(cx);
  pipewire::register(cx);
  power::register(cx);
  clipboard::register(cx);
  mpris::register(cx);
  network::register(cx);
  notifications::register(cx);
  sysinfo::register(cx);
  tray::register(cx);
}
