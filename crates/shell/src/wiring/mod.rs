//! Connects integrations that do not know of each other

use gpui_kit::App;

mod clipboard;
mod idle_session;
mod night_light;
mod notifications;
mod session_control;
mod power_warnings;
mod weather_location;

pub(crate) fn wire_all(cx: &mut App) {
  clipboard::init(cx);
  idle_session::init(cx);
  night_light::init(cx);
  notifications::init(cx);
  session_control::init(cx);
  power_warnings::init(cx);
  weather_location::init(cx);
}
