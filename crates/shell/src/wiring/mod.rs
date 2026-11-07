//! Connects integrations that do not know of each other

use gpui_kit::App;

mod clipboard;
mod notifications;
mod power_warnings;

pub(crate) fn wire_all(cx: &mut App) {
  clipboard::init(cx);
  notifications::init(cx);
  power_warnings::init(cx);
}
