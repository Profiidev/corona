//! `network:*`, `hotspot:*` and `wifi:switch` IPC commands

use gpui_kit::App;

pub mod network;
pub mod radio;

pub(crate) fn register(cx: &mut App) {
  network::register(cx);
  radio::register(cx);
}
