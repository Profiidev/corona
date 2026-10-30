//! Commands that need nothing from the shell

pub mod apps;
pub mod calculator;
pub mod emoji;
pub mod files;
pub mod gallery;
pub mod lookup;
pub mod processes;
pub mod run;
pub mod storage;
mod theme;
pub mod timers;
mod tools;
pub mod web;

use gpui_kit::App;

/// Registers them; needs the compositor and config globals, and the settings pages and
/// control center pages registered, as the commands opening them are made once
pub fn init(cx: &mut App) {
  apps::init(cx);
  calculator::init(cx);
  files::init(cx);
  emoji::init(cx);
  gallery::init(cx);
  lookup::init(cx);
  processes::init(cx);
  run::init(cx);
  storage::init(cx);
  timers::init(cx);
  web::init(cx);
  theme::init(cx);
  tools::init(cx);
}
