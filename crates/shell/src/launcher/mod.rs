//! The launcher's commands that need the shell, registered into `corona_launcher`

mod system;
mod tools;

use gpui_kit::App;

pub fn init(cx: &mut App) {
  corona_launcher::commands::init(cx);
  system::init(cx);
  tools::init(cx);
}
