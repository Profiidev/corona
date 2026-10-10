use std::{
  cell::Cell,
  collections::HashMap,
  fs,
  rc::Rc,
  sync::{Arc, Mutex},
};

use anyhow::{Context as _, Result};
use gpui_kit::{
  AnyView, App, BorrowAppContext, Entity, EntityId, Global, Subscription, Window, transparent_black,
};
use gpui_shell::{
  ShellRoot, ShellRuntime, Watcher,
  policy::{self, Policy},
};

use crate::{
  PLUGIN_STORAGE_FILENAME,
  module::{ModuleExt, PluginRef, Subscriptions, plugin, settings, surface},
  plugin::{manifest::PluginManifest, paths::Paths},
};

pub struct Script {
  root: Entity<ShellRoot>,
  opener: Rc<Cell<Option<EntityId>>>,
  _watcher: Option<Watcher>,
  _subscriptions: Vec<Subscription>,
  _service: Option<ServiceGuard>,
}

/// Marks the plugin's service as running until the service script is dropped
struct ServiceGuard(plugin::Hub);

impl Drop for ServiceGuard {
  fn drop(&mut self) {
    self.0.stop_service();
  }
}

impl Script {
  /// The script inside gpui-shell's root, which hosts its dialogs, sheets and
  /// toasts. Transparent, over the bar's pill or the panel's own shape; a
  /// widget's is as big as its content, so the bar knows where it is
  pub fn view(&self) -> AnyView {
    self.root.clone().into()
  }

  /// The bar widget showing this script, where its panels open
  pub fn set_opener(&self, widget: EntityId) {
    self.opener.set(Some(widget));
  }
}

/// A view a plugin declares in its manifest
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry<'a> {
  Widget(&'a str),
  Panel(&'a str),
  /// Runs in the background, never shown
  Service,
}

/// Runs plugin views. Which plugins there are is up to the plugin manager.
pub struct ScriptManager {
  runtime: Rc<ShellRuntime>,
  paths: Paths,
  plugins: HashMap<String, PluginManifest>,
  /// What the instances of each plugin share, made as the first one loads
  pub(crate) hubs: HashMap<String, plugin::Hub>,
  /// The plugin that sent each notification, until an action is picked on it
  pub(crate) owners: Arc<Mutex<HashMap<u32, String>>>,
  /// Whether notification actions are listened for, from the first `notify`
  pub(crate) listening: bool,
}

impl Global for ScriptManager {}

impl ScriptManager {
  pub fn new(runtime: Rc<ShellRuntime>, paths: Paths) -> Self {
    Self {
      runtime,
      paths,
      plugins: HashMap::new(),
      hubs: HashMap::new(),
      owners: Arc::default(),
      listening: false,
    }
  }

  /// The plugins that run, by id
  pub fn plugins(&self) -> &HashMap<String, PluginManifest> {
    &self.plugins
  }

  pub fn set_plugins(&mut self, plugins: HashMap<String, PluginManifest>) {
    self.hubs.retain(|id, _| plugins.contains_key(id));
    self.plugins = plugins;
  }

  /// The hub of plugin `id`, shared by all its views
  pub(crate) fn hub(&mut self, id: &str, cx: &mut App) -> plugin::Hub {
    self
      .hubs
      .entry(id.to_string())
      .or_insert_with(|| plugin::Hub::new(cx))
      .clone()
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
      Entry::Service => manifest.service.as_ref().map(|s| &s.view),
    }
    .with_context(|| format!("plugin `{id}` has no {entry:?}"))?;

    let data_dir = manager.paths.data(id);
    if let Err(error) = fs::create_dir_all(&data_dir) {
      tracing::warn!("storage unavailable for `{id}`: {error}");
    }

    let runtime = manager.runtime.clone();
    let root = manifest.dir.join(view);
    let (id, name, modules, settings) = (
      manifest.id.clone(),
      manifest.name.clone(),
      manifest.modules.clone(),
      manifest.settings.clone(),
    );
    let panels: Vec<String> = manifest.panels.keys().cloned().collect();
    let opener = Rc::new(Cell::new(None));

    let (policy, mut subscribes) = Policy::new()
      .with_application(&id)
      .with_capabilities(manifest.capabilities.clone())
      .with_storage_path(data_dir.join(PLUGIN_STORAGE_FILENAME))
      .with_corona_modules(
        PluginRef {
          id: &id,
          name: &name,
        },
        &modules,
        cx,
      )?;
    let hub = cx.update_global::<ScriptManager, _>(|manager, cx| manager.hub(&id, cx));
    let service = (entry == Entry::Service).then(|| {
      hub.start_service();
      ServiceGuard(hub.clone())
    });
    let policy = policy
      .with_host_module(settings::module(&id, &settings, &mut subscribes))?
      .with_host_module(surface::module(
        &id,
        panels,
        opener.clone(),
        &Subscriptions::default(),
        &mut subscribes,
        cx,
      ))?
      .with_host_module(plugin::module(&hub, service.is_some(), &mut subscribes))?;

    // The one seam that carries a policy into a view from outside the crate.
    // Reset afterwards so a later load cannot inherit this script's grant.
    policy::set_default(policy);
    let root = runtime.try_load_entry(root, window, cx);
    policy::set_default(Policy::new());
    let root = root?;
    root.update(cx, |root, cx| {
      root.set_background(Some(transparent_black()), cx);
      root.set_fill(!matches!(entry, Entry::Widget(_)), cx);
    });

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
      opener,
      _watcher: watcher,
      _subscriptions: subscriptions,
      _service: service,
    })
  }
}

/// Calls `method` of plugin `id`'s service, like `call` of `corona/plugin`
pub fn call(
  id: &str,
  method: String,
  args: serde_json::Value,
  cx: &mut App,
) -> Result<impl Future<Output = Result<serde_json::Value>> + use<>> {
  if !cx.global::<ScriptManager>().plugins.contains_key(id) {
    anyhow::bail!("plugin `{id}` not found");
  }
  let hub = cx.update_global::<ScriptManager, _>(|manager, cx| manager.hub(id, cx));
  hub.call(method, args, cx)
}

#[cfg(test)]
mod tests {
  use std::{cell::RefCell, path::PathBuf, time::Duration};

  use corona_compositor::{Compositor, CompositorImpl, types};
  use gpui_kit::{self as gpui, AppContext, TestAppContext, VisualTestContext};

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

  /// Shows the loaded views, so they render.
  struct Host(Vec<AnyView>);

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
    let (host, cx) = host(cx, plugins);
    let result = show(cx, &host, id);
    (cx, result)
  }

  /// A window to show views in, with `plugins` running
  fn host<'a>(
    cx: &'a mut TestAppContext,
    plugins: &Plugins,
  ) -> (Entity<Host>, &'a mut VisualTestContext) {
    cx.set_global(plugins.manager());
    if !cx.update(|cx| cx.has_global::<Config>()) {
      cx.set_global(Config::default());
    }
    cx.add_window_view(|_, _| Host(Vec::new()))
  }

  /// Loads view `main` of plugin `id` and shows it in `host`.
  fn show(cx: &mut VisualTestContext, host: &Entity<Host>, id: &str) -> Result<Script> {
    let result = cx.update(|window, cx| ScriptManager::load(id, Entry::Widget("main"), window, cx));
    if let Ok(script) = &result {
      let view = script.view();
      cx.update(|_, cx| {
        host.update(cx, |host, cx| {
          host.0.push(view);
          cx.notify();
        })
      });
    }
    cx.run_until_parked();
    result
  }

  /// Starts the service of plugin `id` in the window, without showing it.
  fn service(cx: &mut VisualTestContext, id: &str) -> Script {
    let script = cx
      .update(|window, cx| ScriptManager::load(id, Entry::Service, window, cx))
      .unwrap();
    cx.run_until_parked();
    script
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
  fn roots_are_transparent_and_widgets_sized_to_content(cx: &mut TestAppContext) {
    let plugins = Plugins::new();
    let extra = "[panels.p]\nview = \"main.js\"";
    plugins.add("a", &manifest("a", extra), &[("main.js", VIEW)]);
    cx.set_global(plugins.manager());
    cx.set_global(Config::default());
    let cx = cx.add_empty_window();
    for (entry, fills) in [(Entry::Widget("main"), false), (Entry::Panel("p"), true)] {
      let script = cx
        .update(|window, cx| ScriptManager::load("a", entry, window, cx))
        .unwrap();
      cx.update(|_, cx| {
        let root = script.root.read(cx);
        assert_eq!(root.background(), Some(transparent_black()), "{entry:?}");
        assert_eq!(root.fills(), fills, "{entry:?}");
      });
    }
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
    fn configure_monitor(&self, _: &str, _: types::MonitorChange) -> anyhow::Result<()> {
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

  /// Reports what `corona/surface` answers through `focusWorkspace` on every
  /// render.
  const SURFACE_VIEW: &str = r#"
import { View } from "gpui-kit";
import { v_flex } from "gpui-base";
import { focusWorkspace } from "corona/compositor";
import { panels, isPanelOpen, togglePanel, openPanel, closePanel, bar } from "corona/surface";

export default class Main extends View {
  render(_cx) {
    // an error comes back as `{ message }`
    const errors = [togglePanel("other"), openPanel("other"), closePanel("other"), isPanelOpen("other")]
      .filter((e) => e && e.message.includes("no panel `other`")).length;
    focusWorkspace(JSON.stringify({
      panels: panels(),
      open: isPanelOpen("p"),
      bar: bar(),
      errors,
      // own panels are fine; there is no bar to open one at here
      toggle: togglePanel("p") ?? null,
      close: closePanel("p") ?? null,
    }));
    return v_flex().child("plugin");
  }
}
"#;

  #[gpui::test]
  fn surface_reads_and_follows_its_panels(cx: &mut TestAppContext) {
    let fake = Rc::new(Fake::default());
    cx.update(|cx| {
      let compositor = Compositor::new(cx, fake.clone()).unwrap();
      cx.set_global(compositor);
      corona_surface::init(cx).unwrap();
    });
    let plugins = Plugins::new();
    let extra = "capabilities = { corona = [\"compositor\"] }\n[panels.p]\nview = \"main.js\"";
    plugins.add("a", &manifest("a", extra), &[("main.js", SURFACE_VIEW)]);
    let (cx, script) = load(cx, &plugins, "a");
    let script = script.unwrap();
    script.set_opener(cx.update(|_, cx| cx.new(|_| ()).entity_id()));
    let last = |fake: &Fake| -> serde_json::Value {
      let calls = fake.calls.borrow();
      serde_json::from_str(calls.last().unwrap()).unwrap()
    };
    assert_eq!(
      last(&fake),
      serde_json::json!({
        "panels": ["p"],
        "open": false,
        // a widget in no bar
        "bar": null,
        "errors": 4,
        "toggle": null,
        "close": null,
      })
    );

    // it asked whether `p` is open, so it renders as that changes
    let rendered = fake.calls.borrow().len();
    cx.update(|_, cx| {
      corona_surface::panel::PanelState::open_panels(cx).update(cx, |open, cx| {
        open.insert("a:p".into());
        cx.notify();
      })
    });
    cx.run_until_parked();
    assert!(fake.calls.borrow().len() > rendered);
    assert_eq!(last(&fake)["open"], true);
  }

  #[gpui::test]
  fn loading_types_the_settings_for_the_editor(cx: &mut TestAppContext) {
    let plugins = Plugins::new();
    let extra = "[[settings]]\nkey = \"units\"\nlabel = \"Units\"\ntype = \"select\"\ndefault = \"a\"\noptions = [{ value = \"a\", label = \"A\" }, { value = \"b\", label = \"B\" }]";
    plugins.add("a", &manifest("a", extra), &[("main.js", VIEW)]);
    let (_, script) = load(cx, &plugins, "a");
    script.unwrap();
    let dts = fs::read_to_string(plugins.paths.local.join("a/gpui-kit.d.ts")).unwrap();
    let start = dts.find("declare module \"corona/settings\"").unwrap();
    let module = &dts[start..start + dts[start..].find("\n}").unwrap()];
    assert!(module.contains("    \"units\": \"a\" | \"b\";"), "{module}");
    assert!(
      module.contains("export function get<K extends keyof Settings>"),
      "{module}"
    );
  }

  /// Installs a compositor that records what it is told to focus, which
  /// views report through.
  fn recorder(cx: &mut TestAppContext) -> Rc<Fake> {
    let fake = Rc::new(Fake::default());
    cx.update(|cx| {
      let compositor = Compositor::new(cx, fake.clone()).unwrap();
      cx.set_global(compositor);
    });
    fake
  }

  fn reported(fake: &Fake, prefix: &str) -> Vec<String> {
    let calls = fake.calls.borrow();
    let found = calls.iter().filter_map(|c| c.strip_prefix(prefix));
    found.map(str::to_string).collect()
  }

  /// Echoes `echo`, never answers `hang`, fails everything else.
  const SERVICE: &str = r#"
import { View } from "gpui-kit";
import { v_flex } from "gpui-base";
import { nextCall, reply, fail } from "corona/plugin";

export default class Service extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      while (true) {
        const call = await nextCall();
        if (call.message) break;
        if (call.method === "echo") reply(call.id, call.args);
        else if (call.method !== "hang") fail(call.id, "no " + call.method);
      }
    });
  }
  render() { return v_flex(); }
}
"#;

  /// Calls `METHOD` as it starts and reports the answer.
  const CALLER: &str = r#"
import { View } from "gpui-kit";
import { v_flex } from "gpui-base";
import { focusWorkspace } from "corona/compositor";
import { call } from "corona/plugin";

export default class Main extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      focusWorkspace("answer " + JSON.stringify(await call("METHOD", 1)));
    });
  }
  render() { return v_flex().child("plugin"); }
}
"#;

  fn caller(plugins: &Plugins, method: &str, service: bool) {
    let extra = match service {
      true => "capabilities = { corona = [\"compositor\"] }\n[service]\nview = \"service.js\"",
      false => "capabilities = { corona = [\"compositor\"] }",
    };
    plugins.add(
      "a",
      &manifest("a", extra),
      &[
        ("main.js", &CALLER.replace("METHOD", method)),
        ("service.js", SERVICE),
      ],
    );
  }

  #[gpui::test]
  fn a_service_answers_without_being_shown(cx: &mut TestAppContext) {
    let fake = recorder(cx);
    let plugins = Plugins::new();
    caller(&plugins, "echo", true);
    let (host, cx) = host(cx, &plugins);
    let _service = service(cx, "a");
    let _view = show(cx, &host, "a").unwrap();
    assert_eq!(reported(&fake, "answer "), ["1"]);
  }

  #[gpui::test]
  fn the_service_fails_what_it_does_not_know(cx: &mut TestAppContext) {
    let fake = recorder(cx);
    let plugins = Plugins::new();
    caller(&plugins, "other", true);
    let (host, cx) = host(cx, &plugins);
    let _service = service(cx, "a");
    let _view = show(cx, &host, "a").unwrap();
    assert_eq!(reported(&fake, "answer "), [r#"{"message":"no other"}"#]);
  }

  #[gpui::test]
  fn calls_without_a_service_fail(cx: &mut TestAppContext) {
    let fake = recorder(cx);
    let plugins = Plugins::new();
    caller(&plugins, "echo", false);
    let (cx, script) = load(cx, &plugins, "a");
    let _script = script.unwrap();
    assert_eq!(
      reported(&fake, "answer "),
      [r#"{"message":"plugin has no running service"}"#]
    );
    // and there is none to start
    let error = cx
      .update(|window, cx| ScriptManager::load("a", Entry::Service, window, cx))
      .err()
      .unwrap();
    assert!(error.to_string().contains("has no Service"), "{error}");
  }

  #[gpui::test]
  fn calls_time_out(cx: &mut TestAppContext) {
    let fake = recorder(cx);
    let plugins = Plugins::new();
    caller(&plugins, "hang", true);
    let (host, cx) = host(cx, &plugins);
    let _service = service(cx, "a");
    let _view = show(cx, &host, "a").unwrap();
    assert!(reported(&fake, "answer ").is_empty());
    cx.executor().advance_clock(Duration::from_secs(31));
    cx.run_until_parked();
    assert_eq!(
      reported(&fake, "answer "),
      [r#"{"message":"the service did not answer in time"}"#]
    );
  }

  #[gpui::test]
  fn stopping_the_service_fails_its_calls(cx: &mut TestAppContext) {
    let fake = recorder(cx);
    let plugins = Plugins::new();
    caller(&plugins, "hang", true);
    let (host, cx) = host(cx, &plugins);
    let service = service(cx, "a");
    let _view = show(cx, &host, "a").unwrap();
    drop(service);
    cx.run_until_parked();
    assert_eq!(
      reported(&fake, "answer "),
      [r#"{"message":"service stopped"}"#]
    );
  }

  #[gpui::test]
  fn ipc_calls_reach_the_service(cx: &mut TestAppContext) {
    recorder(cx);
    let plugins = Plugins::new();
    caller(&plugins, "echo", true);
    let (_, cx) = host(cx, &plugins);
    let _service = service(cx, "a");
    let answer = cx.update(|_, cx| crate::call("a", "echo".into(), "hi".into(), cx).unwrap());
    let answer = cx.executor().spawn(answer);
    cx.run_until_parked();
    assert_eq!(
      futures_lite::future::block_on(answer).unwrap(),
      serde_json::json!("hi")
    );
    let missing = cx.update(|_, cx| crate::call("b", "echo".into(), "hi".into(), cx).err());
    assert!(missing.unwrap().to_string().contains("not found"));
  }

  /// Counts its starts in the shared state and reports what it reads.
  const STATE_VIEW: &str = r#"
import { View } from "gpui-kit";
import { v_flex } from "gpui-base";
import { focusWorkspace } from "corona/compositor";
import { getState, setState, nextCall } from "corona/plugin";

export default class Main extends View {
  init(_props, cx) {
    setState("starts", (getState("starts") ?? 0) + 1);
    cx.spawn(async () => focusWorkspace("next " + JSON.stringify(await nextCall())));
  }
  render() {
    focusWorkspace("starts " + getState("starts"));
    return v_flex().child("plugin");
  }
}
"#;

  #[gpui::test]
  fn state_is_shared_and_rerenders(cx: &mut TestAppContext) {
    let fake = recorder(cx);
    let plugins = Plugins::new();
    let extra = r#"capabilities = { corona = ["compositor"] }"#;
    plugins.add("a", &manifest("a", extra), &[("main.js", STATE_VIEW)]);
    let (host, cx) = host(cx, &plugins);
    let _first = show(cx, &host, "a").unwrap();
    assert_eq!(reported(&fake, "starts ").last().unwrap(), "1");
    let _second = show(cx, &host, "a").unwrap();
    // both views render the second start
    assert!(
      reported(&fake, "starts ")
        .iter()
        .filter(|s| *s == "2")
        .count()
        >= 2
    );

    // only the service takes calls
    assert_eq!(
      reported(&fake, "next "),
      [r#"{"message":"only the plugin's service takes calls"}"#; 2]
    );
  }

  /// Counts its starts in the shared state as it starts.
  const START_COUNTER: &str = r#"
import { View } from "gpui-kit";
import { v_flex } from "gpui-base";
import { focusWorkspace } from "corona/compositor";
import { getState, setState } from "corona/plugin";

export default class Main extends View {
  init() {
    const starts = (getState("starts") ?? 0) + 1;
    setState("starts", starts);
    focusWorkspace("starts " + starts);
  }
  render() { return v_flex(); }
}
"#;

  #[gpui::test]
  fn state_outlives_a_rescan_but_not_the_plugin(cx: &mut TestAppContext) {
    let fake = recorder(cx);
    let plugins = Plugins::new();
    let extra = r#"capabilities = { corona = ["compositor"] }"#;
    plugins.add("a", &manifest("a", extra), &[("main.js", START_COUNTER)]);
    let (_, cx) = host(cx, &plugins);
    let start = |cx: &mut VisualTestContext| {
      let script =
        cx.update(|window, cx| ScriptManager::load("a", Entry::Widget("main"), window, cx));
      drop(script.unwrap());
      cx.run_until_parked();
    };
    let set_plugins = |cx: &mut VisualTestContext, plugins| {
      cx.update(|_, cx| cx.global_mut::<ScriptManager>().set_plugins(plugins))
    };
    let running = cx.update(|_, cx| cx.global::<ScriptManager>().plugins().clone());

    start(cx);
    set_plugins(cx, running.clone());
    start(cx);
    set_plugins(cx, HashMap::new());
    set_plugins(cx, running);
    start(cx);
    assert_eq!(reported(&fake, "starts "), ["1", "2", "1"]);
  }

  #[gpui::test]
  fn every_plugin_has_plugin(cx: &mut TestAppContext) {
    let plugins = Plugins::new();
    plugins.add("a", &manifest("a", ""), &[("main.js", VIEW)]);
    let (_, script) = load(cx, &plugins, "a");
    script.unwrap();
    let dts = fs::read_to_string(plugins.paths.local.join("a/gpui-kit.d.ts")).unwrap();
    for line in [
      "declare module \"corona/plugin\"",
      "export function call(method: string, args?: JsonValue | null): Promise<JsonValue | Error>;",
      "export function nextCall(): Promise<Call | Error>;",
    ] {
      assert!(dts.contains(line), "missing {line:?} in\n{dts}");
    }
  }
}
