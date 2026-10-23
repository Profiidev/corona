//! Commands that need nothing from the shell

pub mod apps;
pub mod calculator;
pub mod clipboard;
pub mod emoji;
pub mod files;
pub mod gallery;
pub mod run;
pub mod snippets;
pub mod web;

use gpui_kit::App;

/// Registers them; needs the compositor and config globals
pub fn init(cx: &mut App) {
  apps::init(cx);
  calculator::init(cx);
  clipboard::init(cx);
  files::init(cx);
  emoji::init(cx);
  gallery::init(cx);
  run::init(cx);
  snippets::init(cx);
  web::init(cx);
}
