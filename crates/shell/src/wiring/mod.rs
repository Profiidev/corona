//! Connects integrations that do not know of each other

use gpui_kit::App;

mod clipboard;
mod idle_session;
mod notifications;
mod session_control;
mod power_warnings;

pub(crate) fn wire_all(cx: &mut App) {
  clipboard::init(cx);
  idle_session::init(cx);
  notifications::init(cx);
  session_control::init(cx);
  power_warnings::init(cx);
}
