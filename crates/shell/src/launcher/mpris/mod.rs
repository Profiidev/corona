//! Media controls and the players as launcher commands

use gpui_kit::App;

mod media;
mod player;

pub(super) fn register(cx: &mut App) {
  media::register(cx);
  player::register(cx);
}
