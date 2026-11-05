//! The compositor's windows, workspaces and monitors in the shell: bar widgets, the window
//! switcher, the taskbar, screen corners, the displays page, their commands and the
//! compositor script modules

use corona_js::ScriptExt;
use corona_utils::Buses;
use gpui_kit::App;

pub mod cli;
pub mod commands;
mod launcher;
pub mod script;
pub mod settings;
pub mod ui;

rust_i18n::i18n!("../../assets/locales", fallback = "en");

/// Registers the windows' UI, commands and script modules; the compositor itself starts
/// before the integrations
pub fn init(cx: &mut App, _: &Buses) {
  ui::register(cx);
  commands::register(cx);
  launcher::register(cx);
  settings::register(cx);
  register_script(cx.script_mut());
}

/// Registers this integration's script modules, for the shell and the CLI
pub fn register_script(registry: &mut corona_js::ScriptRegistry) {
  registry
    .register::<script::CompositorModule>()
    .register::<script::CompositorControlModule>();
}
