//! The launcher's commands that need the shell, registered into `corona_launcher`

mod audio;
pub mod commands;
mod display;
mod info;
mod media;
mod session;
mod system;
mod tools;
mod windows;

use corona_utils::error::ErrorLogExt;
use gpui_kit::App;

pub fn init(cx: &mut App) {
  corona_launcher::init(cx);
  corona_launcher::hooks::set_open_settings(
    |page, cx| {
      let _ = crate::settings::open(page, cx).log_err();
    },
    cx,
  );
  corona_launcher::commands::init(cx);
  session::init(cx);
  system::init(cx);
  audio::init(cx);
  media::init(cx);
  display::init(cx);
  tools::init(cx);
  windows::init(cx);
  info::init(cx);
}
