//! Launcher rows and sources of the integrations

use gpui_kit::App;

mod bluetooth;
mod capture;
mod disks;
mod idle;
mod brightness;
pub(crate) mod clipboard;
mod mpris;
mod network;
mod notifications;
mod pipewire;
mod power;
mod processes;
mod sysinfo;
mod tray;
mod weather;

pub(crate) fn register_all(cx: &mut App) {
  bluetooth::register(cx);
  capture::register(cx);
  disks::register(cx);
  idle::register(cx);
  brightness::register(cx);
  clipboard::register(cx);
  mpris::register(cx);
  network::register(cx);
  notifications::register(cx);
  pipewire::register(cx);
  power::register(cx);
  sysinfo::register(cx);
  tray::register(cx);
  weather::register(cx);
  processes::register(cx);
}
