//! `media:*` and `player:*` IPC commands

use gpui_kit::App;

pub mod media;
pub mod player;

pub(crate) fn register(cx: &mut App) {
  media::register(cx);
  player::register(cx);
}
