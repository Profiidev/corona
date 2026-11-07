//! The compositor's windows, workspaces and monitors in the shell: bar widgets, the window
//! switcher, the taskbar, screen corners, the displays page and their commands

use corona_utils::Buses;
use gpui_kit::App;

pub mod cli;
pub mod commands;
mod launcher;
pub mod settings;
pub mod ui;

rust_i18n::i18n!("../../assets/locales", fallback = "en");

/// Registers the windows' UI and commands; the compositor itself starts
/// before the integrations
pub fn init(cx: &mut App, _: &Buses) {
  ui::register(cx);
  commands::register(cx);
  launcher::register(cx);
  settings::register(cx);
}
