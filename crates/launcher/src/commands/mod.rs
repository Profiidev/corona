//! Commands that need nothing from the shell

pub mod apps;
pub mod calculator;
pub mod run;
pub mod web;

use gpui_kit::App;

/// Registers them; needs the compositor and config globals
pub fn init(cx: &mut App) {
  apps::init(cx);
  calculator::init(cx);
  run::init(cx);
  web::init(cx);
}
