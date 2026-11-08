//! On-screen displays of the integrations

use gpui_kit::App;

pub(crate) mod audio;
mod bluetooth;
mod brightness;
mod idle;
pub(crate) mod keys;
mod network;
mod notifications;
mod power;

pub(crate) fn register_all(cx: &mut App) {
  keys::init(cx);
  audio::init(cx);
  bluetooth::init(cx);
  brightness::init(cx);
  idle::init(cx);
  network::init(cx);
  notifications::init(cx);
  power::init(cx);
}
