use std::path::PathBuf;

use anyhow::{Context as _, Result};
use corona_config::Config;
use gpui_kit::{App, Window};

pub use manager::{Entry, Script, ScriptManager, Service, call};
pub use module::{dbus::Buses, secrets};
pub use plugin::manager::{PluginManager, PluginStatus};

const PLUGIN_MANIFEST_FILENAME: &str = "plugin.toml";
const PLUGIN_STORAGE_FILENAME: &str = "store.json";

mod host_fn;
mod manager;
mod module;
pub mod plugin;

pub fn init(cx: &mut App) -> Result<()> {
  let paths = plugin::paths::Paths::from_config();
  cx.set_global(ScriptManager::new(paths.clone()));
  PluginManager::init(paths, cx);

  Ok(())
}

/// Writes the `gpui-kit.d.ts` of the plugins in `dirs` as loading them does, with no shell
/// or system service running, for `corona types`. The written files.
pub fn write_plugin_types(dirs: &[PathBuf], cx: &mut App) -> Result<Vec<PathBuf>> {
  if !cx.has_global::<Config>() {
    cx.set_global(Config::default());
  }
  if !cx.has_global::<ScriptManager>() {
    // nothing is stored, the plugins never run
    let state = std::env::temp_dir().join("corona-types");
    let local = state.join("local");
    cx.set_global(ScriptManager::new(plugin::paths::Paths { state, local }));
  }
  let mut written = Vec::new();
  for dir in dirs {
    let files = ScriptManager::write_types(dir, cx)
      .with_context(|| format!("plugin in {}", dir.display()))?;
    written.extend(files);
  }
  Ok(written)
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
