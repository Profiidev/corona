//! Fullscreen and popup windows of the integrations

use gpui_kit::App;

pub(crate) mod colorpicker;
pub(crate) mod drive_password;
pub(crate) mod notifications;
pub(crate) mod screenshot;

pub(crate) fn register_all(cx: &mut App) {
  notifications::init(cx);
  drive_password::init(cx);
}
