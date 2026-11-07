//! The IPC commands of the integrations

use gpui_kit::App;

pub mod bluetooth;

pub(crate) fn register_all(cx: &mut App) {
  bluetooth::register(cx);
}
