use std::path::Path;

use anyhow::Result;
use corona_config::{APP_NAME, ConfigProvider};
use gpui_kit::{App, Window};
use gpui_shell::ShellRuntime;

pub use manager::{Script, ScriptManager};

const PLUGIN_MANIFEST_FILENAME: &str = "plugin.json";
const PLUGIN_SCHEMA_FILENAME: &str = "plugin.schema.json";
const PLUGIN_STORAGE_FILENAME: &str = "store.json";

mod host_fn;
mod manager;
mod manifest;
mod module;

pub fn init(cx: &mut App) -> Result<()> {
  let components = gpui_component_shell::components()?;

  #[cfg(debug_assertions)]
  {
    use crate::manifest::ManifestFile;

    let schema = schemars::schema_for!(ManifestFile).to_value();
    let schema_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(PLUGIN_SCHEMA_FILENAME);
    if let Err(error) = std::fs::write(&schema_path, serde_json::to_string_pretty(&schema)?) {
      tracing::debug!("script schema not refreshed: {error}");
    }
  }

  let runtime = ShellRuntime::new_with_components(cx, components)?;
  let data_home = dirs::data_dir()
    .unwrap_or_default()
    .join(APP_NAME)
    .join("plugins");
  let plugin_directory = cx.config().plugin_dir.clone();

  let mut manager = ScriptManager::new(runtime, data_home, plugin_directory);
  manager.discover();
  cx.set_global(manager);

  Ok(())
}

pub trait ScriptManagerExt {
  fn load_plugin_view(&mut self, id: &str, view: &str, window: &mut Window) -> Result<Script>;
}

impl ScriptManagerExt for App {
  fn load_plugin_view(&mut self, id: &str, view: &str, window: &mut Window) -> Result<Script> {
    ScriptManager::load(id, view, window, self)
  }
}
