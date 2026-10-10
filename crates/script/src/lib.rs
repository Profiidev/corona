use anyhow::Result;
use gpui_kit::{App, Window};
use gpui_shell::ShellRuntime;

pub use manager::{Entry, Script, ScriptManager, Service, call};
pub use module::dbus::Buses;
pub use plugin::manager::{PluginManager, PluginStatus};

const PLUGIN_MANIFEST_FILENAME: &str = "plugin.toml";
const PLUGIN_STORAGE_FILENAME: &str = "store.json";

mod host_fn;
mod manager;
mod module;
pub mod plugin;

pub fn init(cx: &mut App) -> Result<()> {
  let components = gpui_component_shell::components()?;

  let runtime = ShellRuntime::new_with_components(cx, components)?;
  let paths = plugin::paths::Paths::from_config();
  cx.set_global(ScriptManager::new(runtime, paths.clone()));
  PluginManager::init(paths, cx);

  Ok(())
}

/// Gives `corona/dbus` corona's bus connections.
pub fn init_dbus(cx: &mut App, system: &zbus::Connection, session: &zbus::Connection) {
  cx.set_global(Buses {
    session: session.clone(),
    system: system.clone(),
  });
}

pub trait ScriptManagerExt {
  fn load_plugin_view(&mut self, id: &str, entry: Entry, window: &mut Window) -> Result<Script>;
}

impl ScriptManagerExt for App {
  fn load_plugin_view(&mut self, id: &str, entry: Entry, window: &mut Window) -> Result<Script> {
    ScriptManager::load(id, entry, window, self)
  }
}
