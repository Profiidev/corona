//! The launcher's commands that need the shell, registered into `corona_launcher`

pub mod commands;

use gpui_kit::App;

pub fn init(cx: &mut App) {
  corona_launcher::init(cx);
}
