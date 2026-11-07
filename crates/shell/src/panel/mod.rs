//! Panels of the integrations: the control center pages

use gpui_kit::App;

pub(crate) mod bluetooth;

pub(crate) fn register_all(cx: &mut App) {
  bluetooth::register(cx);
}
