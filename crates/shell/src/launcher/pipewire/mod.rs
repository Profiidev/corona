//! Volume, outputs and inputs, and streams as launcher commands

use gpui_kit::App;

mod audio;
mod devices;

pub(crate) fn register(cx: &mut App) {
  audio::register(cx);
  devices::register(cx);
}
