//! The lock screen, the polkit dialog and the dashboard's session menu

use gpui_kit::App;

pub mod lock;
mod polkit;
pub mod session;

pub(crate) fn register(cx: &mut App) {
  lock::init(cx);
  polkit::init(cx);
}
