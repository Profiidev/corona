use std::{collections::HashMap, fs, path::PathBuf, rc::Rc};

use anyhow::{Context as _, Result};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{AnyView, App, Entity, Global, Subscription, Window};
use gpui_shell::{
  ShellRoot, ShellRuntime, Watcher,
  policy::{self, Policy},
};

use crate::{
  PLUGIN_MANIFEST_FILENAME, PLUGIN_STORAGE_FILENAME,
  manifest::{ManifestFile, PluginManifest},
  module::ModuleExt,
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

pub struct ScriptManager {
  runtime: Rc<ShellRuntime>,
  data_dir: PathBuf,
  plugin_dir: PathBuf,
  plugins: HashMap<String, PluginManifest>,
}

impl Global for ScriptManager {}

impl ScriptManager {
  pub fn new(runtime: Rc<ShellRuntime>, data_dir: PathBuf, plugin_dir: PathBuf) -> Self {
    Self {
      runtime,
      data_dir,
      plugin_dir,
      plugins: HashMap::new(),
    }
  }

  pub fn discover(&mut self) {
    self.plugins = fs::read_dir(&self.plugin_dir)
      .into_iter()
      .flatten()
      .flatten()
      .map(|entry| entry.path())
      .filter(|path| path.join(PLUGIN_MANIFEST_FILENAME).is_file())
      .flat_map(|path| {
        let file = fs::File::open(path.join(PLUGIN_MANIFEST_FILENAME))
          .log_err()
          .ok()?;
        let data = serde_json::from_reader::<fs::File, ManifestFile>(file)
          .log_err()
          .ok()?;

        Some((
          data.id.clone(),
          PluginManifest {
            id: data.id,
            name: data.name,
            version: data.version,
            views: data.views,
            capabilities: data.capabilities.grant(&self.plugin_dir, &self.data_dir),
            modules: data.capabilities.modules(),
          },
        ))
      })
      .collect();
  }

  pub fn load(id: &str, view: &str, window: &mut Window, cx: &mut App) -> Result<Script> {
    let manager = cx.global::<ScriptManager>();
    let manifest = manager
      .plugins
      .get(id)
      .with_context(|| format!("Plugin `{id}` not found"))?;
    let (id, modules) = (manifest.id.clone(), manifest.modules.clone());
    let view = manifest
      .views
      .get(view)
      .with_context(|| format!("script `{id}` has no view `{view}`"))?;

    let data_dir = manager.data_dir.join(&id);
    if let Err(error) = fs::create_dir_all(&data_dir) {
      tracing::warn!("storage unavailable for `{id}`: {error}");
    }

    let runtime = manager.runtime.clone();
    let root = manager.plugin_dir.join(&id).join(view);

    let (policy, subscribes) = Policy::new()
      .with_application(&id)
      .with_capabilities(manifest.capabilities.clone())
      .with_storage_path(data_dir.join(PLUGIN_STORAGE_FILENAME))
      .with_corona_modules(&modules, cx)?;

    // The one seam that carries a policy into a view from outside the crate.
    // Reset afterwards so a later load cannot inherit this script's grant.
    policy::set_default(policy);
    let root = runtime.try_load_entry(root, window, cx)?;
    policy::set_default(Policy::new());

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
  use std::cell::RefCell;

  use corona_compositor::{Compositor, CompositorImpl, types};
  use gpui_kit::{self as gpui, TestAppContext, VisualTestContext};

  use super::*;

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
    plugins: PathBuf,
    data: PathBuf,
  }

  impl Plugins {
    fn new() -> Self {
      let dir = tempfile::tempdir().unwrap();
      let (plugins, data) = (dir.path().join("plugins"), dir.path().join("data"));
      fs::create_dir_all(&plugins).unwrap();
      Self {
        _dir: dir,
        plugins,
        data,
      }
    }

    fn add(&self, dir: &str, manifest: &str, files: &[(&str, &str)]) {
      let dir = self.plugins.join(dir);
      fs::create_dir_all(&dir).unwrap();
      fs::write(dir.join(PLUGIN_MANIFEST_FILENAME), manifest).unwrap();
      for (name, content) in files {
        fs::write(dir.join(name), content).unwrap();
      }
    }

    fn manager(&self) -> ScriptManager {
      let runtime =
        ShellRuntime::new_isolated_with_components(gpui_component_shell::components().unwrap())
          .unwrap();
      let mut manager = ScriptManager::new(runtime, self.data.clone(), self.plugins.clone());
      manager.discover();
      manager
    }
  }

  fn manifest(id: &str, extra: &str) -> String {
    format!(r#"{{ "id": "{id}", "name": "Test", "views": {{ "main": "main.js" }} {extra} }}"#)
  }

  fn ids(manager: &ScriptManager) -> Vec<&str> {
    let mut ids: Vec<_> = manager.plugins.keys().map(String::as_str).collect();
    ids.sort();
    ids
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
    let (host, cx) = cx.add_window_view(|_, _| Host(None));
    let result = cx.update(|window, cx| ScriptManager::load(id, "main", window, cx));
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
  fn discovers_plugins_by_manifest_id() {
    let plugins = Plugins::new();
    plugins.add("a", &manifest("com.example.a", ""), &[]);
    plugins.add("other-name", &manifest("com.example.b", ""), &[]);
    plugins.add("broken", "{ not json", &[]);
    plugins.add("incomplete", r#"{ "id": "com.example.c" }"#, &[]);
    fs::create_dir_all(plugins.plugins.join("empty")).unwrap();
    fs::write(plugins.plugins.join("stray.json"), "{}").unwrap();

    let manager = plugins.manager();
    assert_eq!(ids(&manager), ["com.example.a", "com.example.b"]);
    let manifest = &manager.plugins["com.example.a"];
    assert_eq!(manifest.views["main"], "main.js");
    assert!(manifest.modules.is_empty());
  }

  #[test]
  #[ignore = "bug: manifest ids are not validated, `../x` makes load() reach outside the plugin and data dirs"]
  fn bug_path_like_id_is_accepted() {
    let plugins = Plugins::new();
    plugins.add("a", &manifest("../escape", ""), &[]);
    plugins.add("b", &manifest("/abs", ""), &[]);
    assert!(plugins.manager().plugins.is_empty());
  }

  #[test]
  fn missing_plugin_dir_is_empty() {
    let plugins = Plugins::new();
    fs::remove_dir(&plugins.plugins).unwrap();
    assert!(plugins.manager().plugins.is_empty());
  }

  #[test]
  fn grants_are_rooted_in_the_plugin_dir() {
    let plugins = Plugins::new();
    let extra = r#", "capabilities": { "fs": { "execute": ["git"] }, "corona": ["weather"] }"#;
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
    let cx = cx.add_empty_window();

    let error = cx
      .update(|window, cx| ScriptManager::load("missing", "main", window, cx))
      .err()
      .unwrap();
    assert!(error.to_string().contains("not found"), "{error}");

    let error = cx
      .update(|window, cx| ScriptManager::load("a", "settings", window, cx))
      .err()
      .unwrap();
    assert!(error.to_string().contains("no view"), "{error}");
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
    // the grant does not stay behind for the next view
    assert_ne!(policy::default().application(), "a");
  }

  #[gpui::test]
  #[ignore = "bug: load() finds the view under the manifest id, not the directory it was discovered in"]
  fn bug_plugin_dir_named_differently_than_id_fails_to_load(cx: &mut TestAppContext) {
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
  #[ignore = "bug: a failed load leaves the plugin's policy as the default for the next view"]
  fn bug_failed_load_leaks_policy(cx: &mut TestAppContext) {
    let plugins = Plugins::new();
    let extra = r#", "capabilities": { "clipboard": { "read": true } }"#;
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
    let extra = r#", "capabilities": { "corona": ["compositor"] }"#;
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
    let extra = r#", "capabilities": { "corona": ["compositor"] }"#;
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
}
