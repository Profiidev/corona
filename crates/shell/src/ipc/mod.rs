//! The IPC commands of the integrations

use gpui_kit::App;

pub mod auth;
pub mod bluetooth;
pub mod brightness;
pub mod capture;
pub mod clipboard;
pub mod disks;
pub mod idle;
pub mod mpris;
pub mod network;
pub mod notifications;
pub mod pipewire;
pub mod power;
pub mod sysinfo;
pub mod tray;
pub mod windows;

pub(crate) fn register_all(cx: &mut App) {
  windows::register(cx);
  auth::register(cx);
  bluetooth::register(cx);
  capture::register(cx);
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

#[cfg(test)]
mod tests {
  use super::*;
  use gpui_kit::{self as gpui, TestAppContext};

  #[gpui::test]
  fn registers_all_ipc_commands(cx: &mut TestAppContext) {
    cx.update(|cx| {
      register_all(cx);
    });
  }
}
