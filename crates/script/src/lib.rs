use std::path::PathBuf;

use anyhow::{Context as _, Result};
use corona_config::Config;
use corona_js::ScriptExt;
use gpui_kit::App;

pub use manager::{Entry, Script, ScriptManager, Service, call};
#[cfg(any(test, feature = "test-support"))]
pub use module::harness;
pub use module::{
  dbus::Buses,
  plugin::{ActionEvent, Hub},
  register as register_modules, secrets,
};
pub use plugin::manager::{PluginManager, PluginStatus};

rust_i18n::i18n!("../../assets/locales", fallback = "en");

const PLUGIN_MANIFEST_FILENAME: &str = "plugin.toml";
/// The TypeScript of what a plugin imports, written next to its manifest
const PLUGIN_TYPES_FILENAME: &str = "corona.d.ts";

pub mod cli;
pub mod commands;
pub mod hooks;
mod host;
mod integrations;
mod manager;
mod module;
pub mod plugin;
pub mod settings;

/// Runs the enabled plugins: their services, bar widgets and panels, the plugins settings
/// page and `plugin:*` IPC commands.
pub fn init(cx: &mut App) {
  register_modules(cx.script_mut());
  let paths = plugin::paths::Paths::from_config();
  cx.set_global(ScriptManager::new(paths.clone()));
  PluginManager::init(paths, cx);
  commands::register(cx);
  settings::register(cx);
  host::init(cx);
}

/// Writes the `corona.d.ts` of the plugins in `dirs` as loading them does, with no shell
/// or system service running, for `corona types`. The written files.
pub fn write_plugin_types(dirs: &[PathBuf], cx: &mut App) -> Result<Vec<PathBuf>> {
  if !cx.has_global::<Config>() {
    cx.set_global(Config::default());
  }
  register_modules(cx.script_mut());
  // nothing is stored, the plugins never run: the throwaway state goes with `tmp`
  let tmp = tempfile::tempdir()?;
  if !cx.has_global::<ScriptManager>() {
    let state = tmp.path().to_path_buf();
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

#[cfg(test)]
pub(crate) mod test_support {
  use std::rc::Rc;

  use corona_compositor::Compositor;
  use corona_surface::testing::{FakeCompositor, workspace};
  use gpui_kit::TestAppContext;

  /// `corona/compositor` as the manager's tests use it, the module itself is in the shell
  /// the workspaces, and focusing one, which views report through
  pub struct TestCompositor;

  /// Switches to workspace `workspace`
  #[specta::specta]
  fn focus_workspace(
    compositor: corona_js::Glob<Compositor>,
    workspace: String,
  ) -> corona_js::Throws<()> {
    compositor.focus_workspace(&workspace).into()
  }

  impl corona_js::ScriptModule for TestCompositor {
    const NAME: &str = "compositor";
    const DOC: &str = "Workspaces, and switching them.";

    fn module(_: &corona_js::PluginRef, m: &mut corona_js::ModuleBuilder, _: &mut gpui_kit::App) {
      use corona_compositor::CompositorExt;
      m.watch(
        "listWorkspaces",
        "",
        |cx| cx.compositor().workspaces.clone(),
        |cx| corona_js::Json(cx.compositor().list_workspaces(cx).to_vec()),
      );
      corona_js::host_module!(m, { sync: [focus_workspace] });
    }
  }

  /// gpui-kit, the default config and a compositor with two workspaces that records what it is
  /// told to focus, which views report through
  pub fn setup(cx: &mut TestAppContext) -> Rc<FakeCompositor> {
    let fake = FakeCompositor {
      workspaces: vec![workspace("1", "DP-1"), workspace("2", "DP-1")],
      ..Default::default()
    };
    corona_surface::testing::setup(fake, cx)
  }

  /// What views reported, oldest first
  pub fn reports(fake: &FakeCompositor) -> Vec<String> {
    let calls = fake.calls.borrow();
    let reports = calls.iter().filter_map(|c| c.strip_prefix("workspace "));
    reports.map(String::from).collect()
  }
}
