//! Settings sections of the integrations

use gpui_kit::App;

mod printing;
mod sysinfo;

pub(crate) fn register_all(cx: &mut App) {
  sysinfo::register(cx);
  printing::register(cx);
}
