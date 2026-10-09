use std::{collections::HashMap, fs, rc::Rc};

use anyhow::{Context as _, Result};
use gpui_kit::{AnyView, App, Entity, Global, Subscription, Window};
use gpui_shell::{
  ShellRoot, ShellRuntime, Watcher,
  policy::{self, Policy},
};

use crate::{
  PLUGIN_STORAGE_FILENAME,
  module::{ModuleExt, settings},
  plugin::{manifest::PluginManifest, paths::Paths},
};

pub struct Script {
  root: Entity<ShellRoot>,
  _watcher: Option<Watcher>,
  _subscriptions: Vec<Subscription>,
}

impl Script {
  pub fn view(&self) -> AnyView {
    self.root.clone().into()
  }
}

/// A view a plugin declares in its manifest
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry<'a> {
  Widget(&'a str),
  Panel(&'a str),
}

/// Runs plugin views. Which plugins there are is up to the plugin manager.
pub struct ScriptManager {
  runtime: Rc<ShellRuntime>,
  paths: Paths,
  plugins: HashMap<String, PluginManifest>,
}

impl Global for ScriptManager {}

impl ScriptManager {
  pub fn new(runtime: Rc<ShellRuntime>, paths: Paths) -> Self {
    Self {
      runtime,
      paths,
      plugins: HashMap::new(),
    }
  }

  /// The plugins that run, by id
  pub fn plugins(&self) -> &HashMap<String, PluginManifest> {
    &self.plugins
  }

  pub fn set_plugins(&mut self, plugins: HashMap<String, PluginManifest>) {
    self.plugins = plugins;
  }

  pub fn load(id: &str, entry: Entry, window: &mut Window, cx: &mut App) -> Result<Script> {
    let manager = cx.global::<ScriptManager>();
    let manifest = manager
      .plugins
      .get(id)
      .with_context(|| format!("Plugin `{id}` not found"))?;
    let view = match entry {
      Entry::Widget(name) => manifest.widgets.get(name).map(|w| &w.view),
      Entry::Panel(name) => manifest.panels.get(name).map(|p| &p.view),
    }
    .with_context(|| format!("plugin `{id}` has no {entry:?}"))?;

    let data_dir = manager.paths.data(id);
    if let Err(error) = fs::create_dir_all(&data_dir) {
      tracing::warn!("storage unavailable for `{id}`: {error}");
    }

    let runtime = manager.runtime.clone();
    let root = manifest.dir.join(view);
    let (id, modules, settings) = (
      manifest.id.clone(),
      manifest.modules.clone(),
      manifest.settings.clone(),
    );

    let (policy, mut subscribes) = Policy::new()
      .with_application(&id)
      .with_capabilities(manifest.capabilities.clone())
      .with_storage_path(data_dir.join(PLUGIN_STORAGE_FILENAME))
      .with_corona_modules(&modules, cx)?;
    let policy = policy.with_host_module(settings::module(&id, &settings, &mut subscribes))?;

    // The one seam that carries a policy into a view from outside the crate.
    // Reset afterwards so a later load cannot inherit this script's grant.
    policy::set_default(policy);
    let root = runtime.try_load_entry(root, window, cx);
    policy::set_default(Policy::new());
    let root = root?;

    let subscriptions = subscribes
      .into_iter()
      .map(|subscription| subscription(&runtime, &root, cx))
      .collect();

    let watcher = match runtime.watch(&root, window, cx) {
      Ok(watcher) => Some(watcher),
      Err(error) => {
        tracing::debug!("`{id}` not watched: {error}");
        None
      }
    };

    Ok(Script {
      root,
      _watcher: watcher,
      _subscriptions: subscriptions,
    })
  }
}

#[cfg(test)]
mod tests {
  use std::{cell::RefCell, path::PathBuf};

  use corona_compositor::{Compositor, CompositorImpl, types};
  use gpui_kit::{self as gpui, TestAppContext, VisualTestContext};

  use corona_config::Config;

  use super::*;
  use crate::{PLUGIN_MANIFEST_FILENAME, plugin::registry};

  const VIEW: &str = r#"
import { View } from "gpui-kit";
import { v_flex } from "gpui-base";

export default class Main extends View {
  render(_cx) {
    return v_flex().child("plugin");
  }
}
"#;

  /// Focuses workspace `rendered` on every render, after reading the workspaces.
  const COMPOSITOR_VIEW: &str = r#"
import { View } from "gpui-kit";
import { v_flex } from "gpui-base";
import { listWorkspaces, focusWorkspace } from "corona/compositor";

export default class Main extends View {
  render(_cx) {
    const count = listWorkspaces().length;
    focusWorkspace("rendered " + count);
    return v_flex().child("plugin");
  }
}
"#;

  struct Plugins {
    _dir: tempfile::TempDir,
    paths: Paths,
    data: PathBuf,
  }

  impl Plugins {
    fn new() -> Self {
      let dir = tempfile::tempdir().unwrap();
      let paths = Paths {
        state: dir.path().join("state"),
        local: dir.path().join("local"),
      };
      fs::create_dir_all(&paths.local).unwrap();
      Self {
        data: paths.state.join("data"),
        _dir: dir,
        paths,
      }
    }

    fn add(&self, dir: &str, manifest: &str, files: &[(&str, &str)]) {
      let dir = self.paths.local.join(dir);
      fs::create_dir_all(&dir).unwrap();
      fs::write(dir.join(PLUGIN_MANIFEST_FILENAME), manifest).unwrap();
      for (name, content) in files {
        fs::write(dir.join(name), content).unwrap();
      }
    }

    /// Every plugin in the local directory, all enabled
    fn manager(&self) -> ScriptManager {
      let runtime =
        ShellRuntime::new_isolated_with_components(gpui_component_shell::components().unwrap())
          .unwrap();
      let mut manager = ScriptManager::new(runtime, self.paths.clone());
      let root = &registry::roots(&self.paths, &Default::default())[1];
      manager.set_plugins(
        registry::scan_root(&self.paths, root)
          .into_iter()
          .map(|found| (found.manifest.id.clone(), found.manifest))
          .collect(),
      );
      manager
    }
  }

  fn manifest(id: &str, extra: &str) -> String {
    format!("id = \"{id}\"\nname = \"Test\"\n{extra}\n[widgets.main]\nview = \"main.js\"\n")
  }

  /// Shows the loaded view, so it renders.
  struct Host(Option<AnyView>);

  impl gpui::Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
      use gpui::ParentElement as _;
      gpui::div().children(self.0.clone())
    }
  }

  /// Loads view `main` of plugin `id` into a window of its own.
  fn load<'a>(
    cx: &'a mut TestAppContext,
    plugins: &Plugins,
    id: &str,
  ) -> (&'a mut VisualTestContext, Result<Script>) {
    cx.set_global(plugins.manager());
    if !cx.update(|cx| cx.has_global::<Config>()) {
      cx.set_global(Config::default());
    }
    let (host, cx) = cx.add_window_view(|_, _| Host(None));
    let result = cx.update(|window, cx| ScriptManager::load(id, Entry::Widget("main"), window, cx));
    if let Ok(script) = &result {
      let view = script.view();
      cx.update(|_, cx| {
        host.update(cx, |host, cx| {
          host.0 = Some(view);
          cx.notify();
        })
      });
    }
    cx.run_until_parked();
    (cx, result)
  }

  #[test]
  fn grants_are_rooted_in_the_plugin_dir() {
    let plugins = Plugins::new();
    let extra = r#"capabilities = { fs = { execute = ["git"] }, corona = ["weather"] }"#;
    plugins.add("a", &manifest("a", extra), &[]);
    let manager = plugins.manager();
    let manifest = &manager.plugins["a"];
    assert!(manifest.capabilities.may_run("git"));
    assert_eq!(
      manifest.modules,
      [crate::module::CoronaModule::Weather].into()
    );
  }

  #[gpui::test]
  fn load_errors(cx: &mut TestAppContext) {
    let plugins = Plugins::new();
    plugins.add("a", &manifest("a", ""), &[("main.js", VIEW)]);
    cx.set_global(plugins.manager());
    cx.set_global(Config::default());
    let cx = cx.add_empty_window();

    let error = cx
      .update(|window, cx| ScriptManager::load("missing", Entry::Widget("main"), window, cx))
      .err()
      .unwrap();
    assert!(error.to_string().contains("not found"), "{error}");

    for entry in [Entry::Widget("other"), Entry::Panel("main")] {
      let error = cx
        .update(|window, cx| ScriptManager::load("a", entry, window, cx))
        .err()
        .unwrap();
      assert!(error.to_string().contains("has no"), "{error}");
    }
  }

  #[gpui::test]
  fn loads_a_view(cx: &mut TestAppContext) {
    let plugins = Plugins::new();
    plugins.add("a", &manifest("a", ""), &[("main.js", VIEW)]);
    let (cx, script) = load(cx, &plugins, "a");
    let script = script.unwrap();
    cx.run_until_parked();

    let _ = script.view();
    // storage lives in the data dir, under the id
    assert!(plugins.data.join("a").is_dir());
    assert!(!plugins.paths.local.join("a/store.json").exists());
    // the grant does not stay behind for the next view
    assert_ne!(policy::default().application(), "a");
  }

  #[gpui::test]
  fn loads_from_the_discovered_dir(cx: &mut TestAppContext) {
    let plugins = Plugins::new();
    plugins.add(
      "folder",
      &manifest("com.example.a", ""),
      &[("main.js", VIEW)],
    );
    let (_, script) = load(cx, &plugins, "com.example.a");
    script.unwrap();
  }

  #[gpui::test]
  fn failed_load_resets_policy(cx: &mut TestAppContext) {
    let plugins = Plugins::new();
    let extra = r#"capabilities = { clipboard = { read = true } }"#;
    // the view file is missing, so mounting it fails
    plugins.add("a", &manifest("a", extra), &[]);
    let (_, script) = load(cx, &plugins, "a");
    assert!(script.is_err());
    assert!(!policy::default().capabilities().is_clipboard_readable());
  }

  /// Answers from fixed state, records what it was asked to do.
  #[derive(Default)]
  struct Fake {
    calls: RefCell<Vec<String>>,
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
  }

  #[gpui::test]
  fn corona_modules_are_callable_and_rerender(cx: &mut TestAppContext) {
    let fake = Rc::new(Fake::default());
    cx.update(|cx| {
      let compositor = Compositor::new(cx, fake.clone()).unwrap();
      cx.set_global(compositor);
    });
    let plugins = Plugins::new();
    let extra = r#"capabilities = { corona = ["compositor"] }"#;
    plugins.add("a", &manifest("a", extra), &[("main.js", COMPOSITOR_VIEW)]);
    let (cx, script) = load(cx, &plugins, "a");
    let script = script.unwrap();
    let rendered = fake.calls.borrow().len();
    assert!(rendered > 0, "the view rendered");
    assert_eq!(fake.calls.borrow()[0], "rendered 2");

    // the view read the workspaces, so a change renders it again
    let workspaces = cx.update(|_, cx| cx.global::<Compositor>().workspaces.clone());
    cx.update(|_, cx| {
      workspaces.update(cx, |list, cx| {
        list.pop();
        cx.notify();
      })
    });
    cx.run_until_parked();
    assert!(fake.calls.borrow().len() > rendered);
    assert_eq!(fake.calls.borrow().last().unwrap(), "rendered 1");
    drop(script);
  }

  /// Calls every compositor function, then reports the results through `focusWorkspace`.
  const COMPOSITOR_CALLS_VIEW: &str = r#"
import { View } from "gpui-kit";
import { v_flex } from "gpui-base";
import * as c from "corona/compositor";

export default class Main extends View {
  render(_cx) {
    const results = {
      workspaces: c.listWorkspaces().map((w) => w.name),
      activeWorkspace: c.activeWorkspace().name,
      monitors: c.listMonitors().length,
      activeMonitor: c.activeMonitor().name,
      windows: c.listWindows().length,
      activeWindow: c.activeWindow(),
      keyboardLayout: c.keyboardLayout(),
      focusWindow: c.focusWindow("0xa"),
      closeWindow: c.closeWindow("0xb"),
      cursor: c.cursorPosition(),
    };
    c.focusWorkspace(JSON.stringify(results));
    return v_flex().child("plugin");
  }
}
"#;

  #[gpui::test]
  fn calls_every_compositor_function(cx: &mut TestAppContext) {
    let fake = Rc::new(Fake::default());
    cx.update(|cx| {
      let compositor = Compositor::new(cx, fake.clone()).unwrap();
      cx.set_global(compositor);
    });
    let plugins = Plugins::new();
    let extra = r#"capabilities = { corona = ["compositor"] }"#;
    plugins.add(
      "a",
      &manifest("a", extra),
      &[("main.js", COMPOSITOR_CALLS_VIEW)],
    );
    let (_, script) = load(cx, &plugins, "a");
    let _script = script.unwrap();

    let calls = fake.calls.borrow();
    let results: serde_json::Value = serde_json::from_str(calls.last().unwrap()).unwrap();
    assert_eq!(results["workspaces"], serde_json::json!(["1", "2"]));
    assert_eq!(results["activeWorkspace"], "1");
    assert_eq!(results["monitors"], 0);
    assert_eq!(results["activeMonitor"], "DP-1");
    assert_eq!(results["windows"], 0);
    assert!(results["activeWindow"].is_null());
    assert!(results["keyboardLayout"].is_null());
    // `Ok(())` is null
    assert!(results["focusWindow"].is_null());
    assert!(results["closeWindow"].is_null());
    assert_eq!(results["cursor"], serde_json::json!({ "x": 3, "y": 4 }));
  }

  #[gpui::test]
  fn ungranted_corona_modules_cannot_be_imported(cx: &mut TestAppContext) {
    let fake = Rc::new(Fake::default());
    cx.update(|cx| {
      let compositor = Compositor::new(cx, fake.clone()).unwrap();
      cx.set_global(compositor);
    });
    let plugins = Plugins::new();
    plugins.add("a", &manifest("a", ""), &[("main.js", COMPOSITOR_VIEW)]);
    let (_, script) = load(cx, &plugins, "a");
    let error = script.err().expect("the import fails");
    assert!(
      format!("{error:#}").contains("corona/compositor"),
      "{error:#}"
    );
    assert!(fake.calls.borrow().is_empty());
  }

  /// Reports its `label` setting through `focusWorkspace` on every render.
  const SETTINGS_VIEW: &str = r#"
import { View } from "gpui-kit";
import { v_flex } from "gpui-base";
import { focusWorkspace } from "corona/compositor";
import { get, all } from "corona/settings";

export default class Main extends View {
  render(_cx) {
    focusWorkspace(get("label") + " " + Object.keys(all()).length + " " + get("missing"));
    return v_flex().child("plugin");
  }
}
"#;

  #[gpui::test]
  fn settings_are_read_and_rerender(cx: &mut TestAppContext) {
    let fake = Rc::new(Fake::default());
    cx.update(|cx| {
      let compositor = Compositor::new(cx, fake.clone()).unwrap();
      cx.set_global(compositor);
    });
    cx.set_global(Config::default());
    let plugins = Plugins::new();
    let extra = r#"capabilities = { corona = ["compositor"] }
[[settings]]
key = "label"
label = "Label"
type = "text"
default = "hello""#;
    plugins.add("a", &manifest("a", extra), &[("main.js", SETTINGS_VIEW)]);
    let (cx, script) = load(cx, &plugins, "a");
    let _script = script.unwrap();
    assert_eq!(fake.calls.borrow().last().unwrap(), "hello 1 null");

    let rendered = fake.calls.borrow().len();
    // another plugin's settings change nothing
    cx.update(|_, cx| {
      cx.global_mut::<Config>().plugin_settings.insert(
        "b".into(),
        serde_json::json!({ "label": "x" })
          .as_object()
          .unwrap()
          .clone(),
      );
    });
    cx.run_until_parked();
    assert_eq!(fake.calls.borrow().len(), rendered);

    cx.update(|_, cx| {
      cx.global_mut::<Config>().plugin_settings.insert(
        "a".into(),
        serde_json::json!({ "label": "bye" })
          .as_object()
          .unwrap()
          .clone(),
      );
    });
    cx.run_until_parked();
    assert_eq!(fake.calls.borrow().last().unwrap(), "bye 1 null");

    // a value of the wrong type reads as the default
    cx.update(|_, cx| {
      cx.global_mut::<Config>().plugin_settings.insert(
        "a".into(),
        serde_json::json!({ "label": 3 })
          .as_object()
          .unwrap()
          .clone(),
      );
    });
    cx.run_until_parked();
    assert_eq!(fake.calls.borrow().last().unwrap(), "hello 1 null");
  }
}
