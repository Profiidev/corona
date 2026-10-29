//! The launcher's commands that need the shell, registered into `corona_launcher`

mod monitors;
mod system;
mod tools;
mod windows;

use gpui_kit::App;

pub fn init(cx: &mut App) {
  corona_launcher::commands::init(cx);
  system::init(cx);
  tools::init(cx);
  windows::init(cx);
  windows::init_actions(cx);
  monitors::init(cx);
}
