//! On-screen displays of the integrations

use gpui_kit::App;

mod bluetooth;

pub(crate) fn register_all(cx: &mut App) {
  bluetooth::init(cx);
}
