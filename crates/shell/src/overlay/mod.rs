//! Fullscreen and popup windows of the integrations

use gpui_kit::App;

pub(crate) mod notifications;

pub(crate) fn register_all(cx: &mut App) {
  notifications::init(cx);
}
