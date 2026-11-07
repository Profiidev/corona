//! `audio:*` and `volume:*` IPC commands

use gpui_kit::App;

pub mod audio;
pub mod volume;

pub(crate) fn register(cx: &mut App) {
  audio::register(cx);
  volume::register(cx);
}
