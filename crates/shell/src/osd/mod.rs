//! On-screen displays of the integrations

use gpui_kit::App;

mod bluetooth;
mod notifications;

pub(crate) fn register_all(cx: &mut App) {
  bluetooth::init(cx);
  notifications::init(cx);
}
