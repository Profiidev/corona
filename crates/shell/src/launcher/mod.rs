//! Launcher rows and sources of the integrations

use gpui_kit::App;

mod bluetooth;
mod processes;
mod sysinfo;

pub(crate) fn register_all(cx: &mut App) {
  bluetooth::register(cx);
  sysinfo::register(cx);
  processes::register(cx);
}
