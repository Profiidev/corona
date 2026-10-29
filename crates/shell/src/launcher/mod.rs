//! The launcher's commands that need the shell, registered into `corona_launcher`

mod display;
mod drives;
mod ip;
mod monitors;
mod network;
mod session;
mod system;
mod tools;
mod windows;

use gpui_kit::App;

pub fn init(cx: &mut App) {
  corona_launcher::commands::init(cx);
  session::init(cx);
  system::init(cx);
  display::init(cx);
  tools::init(cx);
  windows::init(cx);
  windows::init_actions(cx);
  network::init(cx);
  drives::init(cx);
  monitors::init(cx);
  ip::init(cx);
}
