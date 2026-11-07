//! On-screen displays of the integrations

use gpui_kit::App;

pub(crate) mod audio;
mod bluetooth;
mod notifications;

pub(crate) fn register_all(cx: &mut App) {
  audio::init(cx);
  bluetooth::init(cx);
  notifications::init(cx);
}
