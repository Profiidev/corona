//! `brightness:*` and `nightlight:*` IPC commands

use gpui_kit::App;

pub mod brightness;
pub mod nightlight;

pub(crate) fn register(cx: &mut App) {
  brightness::register(cx);
  nightlight::register(cx);
}
