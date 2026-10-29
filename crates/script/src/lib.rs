use std::path::PathBuf;

use anyhow::{Context as _, Result};
use corona_config::Config;
use corona_js::ScriptExt;
use gpui_kit::{App, Window};

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
const PLUGIN_TYPES_FILENAME: &str = "gpui-kit.d.ts";

pub mod commands;
pub mod hooks;
mod host;
mod manager;
mod module;
pub mod plugin;
pub mod settings;

/// Runs the enabled plugins: their services, bar widgets and panels, the plugins settings
/// page and `plugin:*` IPC commands.
pub fn init(cx: &mut App) -> Result<()> {
  register_modules(cx.script_mut());
  let paths = plugin::paths::Paths::from_config();
  cx.set_global(ScriptManager::new(paths.clone()));
  PluginManager::init(paths, cx);
  commands::register(cx);
  settings::register(cx);
  host::init(cx);
  Ok(())
}

/// Writes the `gpui-kit.d.ts` of the plugins in `dirs` as loading them does, with no shell
/// or system service running, for `corona types`. The written files.
pub fn write_plugin_types(dirs: &[PathBuf], cx: &mut App) -> Result<Vec<PathBuf>> {
  if !cx.has_global::<Config>() {
    cx.set_global(Config::default());
  }
  register_modules(cx.script_mut());
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

#[cfg(test)]
pub(crate) mod test_support {
  use std::{cell::RefCell, rc::Rc};

  use corona_compositor::{Compositor, CompositorImpl, types};
  use gpui_kit::TestAppContext;

  /// Answers from fixed state, records what it was asked to do.
  #[derive(Default)]
  pub struct Fake {
    pub calls: RefCell<Vec<String>>,
  }

  fn workspace(name: &str) -> types::Workspace {
    types::Workspace {
      id: name.into(),
      name: name.into(),
      monitor: "DP-1".into(),
      monitor_id: 0,
    }
  }

  impl CompositorImpl for Fake {
    fn list_workspaces(&self) -> anyhow::Result<Vec<types::Workspace>> {
      Ok(vec![workspace("1"), workspace("2")])
    }
    fn active_workspace(&self) -> anyhow::Result<types::Workspace> {
      Ok(workspace("1"))
    }
    fn list_monitors(&self) -> anyhow::Result<Vec<types::Monitor>> {
      Ok(vec![])
    }
    fn active_monitor(&self) -> anyhow::Result<types::Monitor> {
      Ok(types::Monitor {
        id: 0,
        name: "DP-1".into(),
        width: 1,
        height: 1,
        refresh_rate: 60.,
        x: 0,
        y: 0,
        active_scratchpad: None,
        active_workspace: workspace("1"),
        scale: 1.,
        focused: true,
        disabled: false,
        mirror_of: "none".into(),
      })
    }
    fn list_windows(&self) -> anyhow::Result<Vec<types::Window>> {
      Ok(vec![])
    }
    fn active_window(&self) -> anyhow::Result<Option<types::Window>> {
      Ok(None)
    }
    fn focus_workspace(&self, workspace: &str) -> anyhow::Result<()> {
      self.calls.borrow_mut().push(workspace.into());
      Ok(())
    }
    fn focus_window(&self, _: &str) -> anyhow::Result<()> {
      Ok(())
    }
    fn close_window(&self, _: &str) -> anyhow::Result<()> {
      Ok(())
    }
    fn cursor_position(&self) -> anyhow::Result<(i32, i32)> {
      Ok((3, 4))
    }
    fn keyboard_layout(&self) -> anyhow::Result<Option<String>> {
      Ok(None)
    }
    fn set_dpms(&self, _: bool) -> anyhow::Result<()> {
      Ok(())
    }
    fn configure_monitor(&self, _: &str, _: types::MonitorChange) -> anyhow::Result<()> {
      Ok(())
    }
  }

  /// gpui-kit, the default config and a compositor that records what it is told to focus,
  /// which views report through
  pub fn setup(cx: &mut TestAppContext) -> Rc<Fake> {
    let fake = Rc::new(Fake::default());
    cx.update(|cx| {
      gpui_kit::init(cx);
      if !cx.has_global::<corona_config::Config>() {
        cx.set_global(corona_config::Config::default());
      }
      let compositor = Compositor::new(cx, fake.clone()).unwrap();
      cx.set_global(compositor);
    });
    fake
  }
}
