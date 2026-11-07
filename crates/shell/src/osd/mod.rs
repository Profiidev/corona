//! On-screen displays of the integrations

use gpui_kit::App;

pub(crate) mod audio;
mod bluetooth;
mod brightness;
mod notifications;
mod power;

pub(crate) fn register_all(cx: &mut App) {
  audio::init(cx);
  bluetooth::init(cx);
  brightness::init(cx);
  notifications::init(cx);
  power::init(cx);
}
