//! `brightness:*` and `nightlight:*` IPC commands

use gpui_kit::App;

pub mod display;
pub mod nightlight;

pub(crate) fn register(cx: &mut App) {
  display::register(cx);
  nightlight::register(cx);
}
