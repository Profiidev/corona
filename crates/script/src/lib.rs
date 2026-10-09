#[cfg(debug_assertions)]
use std::path::Path;

use anyhow::Result;
use gpui_kit::{App, Window};
use gpui_shell::ShellRuntime;

pub use manager::{Entry, Script, ScriptManager};
pub use plugin::manager::{PluginManager, PluginStatus};

const PLUGIN_MANIFEST_FILENAME: &str = "plugin.toml";
#[cfg(debug_assertions)]
const PLUGIN_SCHEMA_FILENAME: &str = "plugin.schema.json";
const PLUGIN_STORAGE_FILENAME: &str = "store.json";

mod host_fn;
mod manager;
mod module;
pub mod plugin;

pub fn init(cx: &mut App) -> Result<()> {
  let components = gpui_component_shell::components()?;

  #[cfg(debug_assertions)]
  {
    use crate::plugin::manifest::ManifestFile;

    let schema = schemars::schema_for!(ManifestFile).to_value();
    let schema_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(PLUGIN_SCHEMA_FILENAME);
    if let Err(error) = std::fs::write(&schema_path, serde_json::to_string_pretty(&schema)?) {
      tracing::debug!("script schema not refreshed: {error}");
    }
  }

  let runtime = ShellRuntime::new_with_components(cx, components)?;
  let paths = plugin::paths::Paths::from_config();
  cx.set_global(ScriptManager::new(runtime, paths.clone()));
  PluginManager::init(paths, cx);

  Ok(())
}

pub trait ScriptManagerExt {
  fn load_plugin_view(&mut self, id: &str, entry: Entry, window: &mut Window) -> Result<Script>;
}

impl ScriptManagerExt for App {
  fn load_plugin_view(&mut self, id: &str, entry: Entry, window: &mut Window) -> Result<Script> {
    ScriptManager::load(id, entry, window, self)
  }
}
