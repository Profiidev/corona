//! `network:*`, `hotspot:*` and `wifi:switch` IPC commands

use gpui_kit::App;

pub mod connection;
pub mod radio;

pub(crate) fn register(cx: &mut App) {
  connection::register(cx);
  radio::register(cx);
}
