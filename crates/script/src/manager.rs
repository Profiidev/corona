use std::{
  cell::Cell,
  collections::HashMap,
  fs,
  path::{Path, PathBuf},
  rc::Rc,
  sync::{Arc, Mutex},
};

use anyhow::{Context as _, Result};
use corona_js::{
  GpuiKit, ModuleBuilder, PluginRef, STDLIB_D_TS, ScriptRegistry, ScriptRoot, ScriptView, Watches,
  d_ts, reload_view,
};
use gpui_kit::{
  AnyView, App, AppContext, Context, Entity, EntityId, Global, Task, Window, transparent_black,
};

use crate::{
  PLUGIN_TYPES_FILENAME,
  module::{
    dbus, i18n,
    plugin::{self, Hub},
    settings,
    surface::{self, Own},
  },
  plugin::{
    manifest::{ManifestFile, PluginManifest},
    paths::Paths,
  },
};

pub struct Script {
  root: Entity<ScriptRoot>,
  opener: Rc<Cell<Option<EntityId>>>,
  _reload: Task<()>,
}

impl Script {
  /// The script's root, which a window's `Root` hosts dialogs, sheets and toasts for.
  /// Transparent, over the bar's pill or the panel's own shape; a widget's is as big as its
  /// content, so the bar knows where it is
  pub fn view(&self) -> AnyView {
    self.root.clone().into()
  }

  /// The bar widget showing this script, where its panels open
  pub fn set_opener(&self, widget: EntityId) {
    self.opener.set(Some(widget));
  }
}

/// A plugin's service, running headless until dropped. Restarts as its sources change.
pub struct Service {
  _watch: Task<()>,
}

/// A view a plugin declares in its manifest
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Entry<'a> {
  /// With this instance's options from the bar config
  Widget(&'a str, Option<&'a serde_json::Value>),
  Panel(&'a str),
}

/// What a runtime gets beyond its plugin's grants
#[derive(Clone, Default)]
struct Load {
  opener: Rc<Cell<Option<EntityId>>>,
  options: Option<serde_json::Value>,
  service: bool,
}

/// Runs plugin views and services. Which plugins there are is up to the plugin manager.
pub struct ScriptManager {
  paths: Paths,
  plugins: HashMap<String, PluginManifest>,
  /// What the instances of each plugin share, made as the first one loads
  pub(crate) hubs: HashMap<String, Hub>,
  /// The plugin that sent each notification and whether it is resident, until an action is
  /// picked on a non-resident one
  pub(crate) owners: Arc<Mutex<HashMap<u32, (String, bool)>>>,
  /// Whether notification actions are listened for, from the first `notify`
  pub(crate) listening: bool,
}

impl Global for ScriptManager {}

/// What a failed load hands gpui, which needs a view to make one
const EMPTY: &str = "import { View, div } from \"gpui-kit\";
export default class Empty extends View { render() { return div(); } }";

impl ScriptManager {
  pub fn new(paths: Paths) -> Self {
    Self {
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

  /// Every load runs in a fresh runtime, so an updated plugin's next load runs the update
  pub fn set_plugins(&mut self, plugins: HashMap<String, PluginManifest>) {
    self.hubs.retain(|id, _| plugins.contains_key(id));
    self.plugins = plugins;
  }

  /// Tells plugin `id`'s scripts that its secret `key` was stored or removed
  pub fn secret_changed(id: &str, key: &str, cx: &mut App) {
    let hub = cx
      .try_global::<Self>()
      .and_then(|m| m.hubs.get(id).cloned());
    if let Some(hub) = hub {
      hub.secret_changed(key.into(), cx);
    }
  }

  /// The hub of plugin `id`, shared by all its views
  pub(crate) fn hub(id: &str, cx: &mut App) -> Hub {
    if let Some(hub) = cx.global::<Self>().hubs.get(id) {
      return hub.clone();
    }
    let settings = (cx.global::<Self>().plugins.get(id)).map(|m| m.settings.clone());
    let hub = Hub::new(id, &settings.unwrap_or_default(), cx);
    cx.global_mut::<Self>().hubs.insert(id.into(), hub.clone());
    hub
  }

  fn manifest(id: &str, cx: &App) -> Result<PluginManifest> {
    let manifest = cx.global::<ScriptManager>().plugins.get(id);
    manifest
      .cloned()
      .with_context(|| format!("Plugin `{id}` not found"))
  }

  /// The plugin a runtime runs, with its storage directory made
  fn plugin(manifest: &PluginManifest, cx: &App) -> Rc<PluginRef> {
    let data_dir = cx.global::<ScriptManager>().paths.data(&manifest.id);
    if let Err(error) = fs::create_dir_all(&data_dir) {
      tracing::warn!("storage unavailable for `{}`: {error}", manifest.id);
    }
    Rc::new(PluginRef {
      id: manifest.id.clone(),
      name: manifest.name.clone(),
      dir: manifest.dir.clone(),
      data_dir,
      // a clone shares what the user picks with `corona/desktop`'s
      capabilities: manifest.capabilities.clone(),
    })
  }

  /// The modules of a runtime of plugin `manifest`: the granted ones, which must be
  /// registered, and the ones every plugin has
  fn modules(
    manifest: &PluginManifest,
    plugin: &PluginRef,
    watches: &Watches,
    load: Load,
    cx: &mut App,
  ) -> Result<Vec<ModuleBuilder>> {
    let registry = cx.try_global::<ScriptRegistry>();
    let granted = (manifest.modules.iter())
      .map(|name| {
        (registry.and_then(|r| r.get(name))).with_context(|| {
          format!(
            "plugin `{}` is granted unknown module `{name}`",
            manifest.id
          )
        })
      })
      .collect::<Result<Vec<_>>>()?;
    let mut modules: Vec<_> = (granted.into_iter())
      .map(|module| module.build(plugin, watches, cx))
      .collect();

    let hub = Self::hub(&manifest.id, cx);
    let mut module = |name: &str, build: &dyn Fn(&mut ModuleBuilder)| {
      let mut m = ModuleBuilder::new(format!("corona/{name}"), watches);
      build(&mut m);
      modules.push(m);
    };
    module("settings", &|m| {
      let (id, settings) = (&manifest.id, &manifest.settings);
      settings::module(
        id,
        settings,
        &manifest.host_settings,
        hub.settings.clone(),
        m,
      )
    });
    module("surface", &|m| {
      let own = Own {
        id: manifest.id.clone(),
        panels: manifest.panels.keys().cloned().collect(),
        opener: load.opener.clone(),
        options: load.options.clone(),
      };
      surface::module(own, m)
    });
    module("plugin", &|m| plugin::module(&hub, load.service, m));
    module("i18n", &|m| {
      i18n::module(&manifest.dir, hub.settings.clone(), m)
    });
    if let Some(grant) = &manifest.dbus {
      module("dbus", &|m| dbus::module(grant.clone(), m));
    }
    Ok(modules)
  }

  /// Writes the `gpui-kit.d.ts` of the plugin in `dir` as loading it does, typing its own
  /// settings and granted modules, without a running shell
  pub fn write_types(dir: &Path, cx: &mut App) -> Result<Vec<PathBuf>> {
    let file = ManifestFile::read(dir)?;
    let data = cx.global::<ScriptManager>().paths.data(&file.id);
    let manifest = PluginManifest::new(file, dir.to_path_buf(), &data, None);
    let had_hub = cx.global::<ScriptManager>().hubs.contains_key(&manifest.id);
    let plugin = Self::plugin(&manifest, cx);
    let modules = Self::modules(&manifest, &plugin, &Watches::default(), Load::default(), cx);
    if !had_hub {
      cx.global_mut::<ScriptManager>().hubs.remove(&manifest.id);
    }
    Ok(vec![write_d_ts(dir, &modules?)?])
  }

  pub fn load(id: &str, entry: Entry, window: &mut Window, cx: &mut App) -> Result<Script> {
    let manifest = Self::manifest(id, cx)?;
    let view = match entry {
      Entry::Widget(name, _) => manifest.widgets.get(name).map(|w| &w.view),
      Entry::Panel(name) => manifest.panels.get(name).map(|p| &p.view),
    }
    .with_context(|| format!("plugin `{id}` has no {entry:?}"))?;

    let path = manifest.dir.join(view);
    let dir = manifest.dir.clone();
    let opener = Rc::new(Cell::new(None));
    let load = Load {
      opener: opener.clone(),
      options: match entry {
        Entry::Widget(_, options) => options.cloned(),
        Entry::Panel(_) => None,
      },
      service: false,
    };
    let make = move |window: &mut Window, cx: &mut Context<ScriptView>| {
      let plugin = Self::plugin(&manifest, cx);
      let watches = Watches::default();
      let modules = Self::modules(&manifest, &plugin, &watches, load.clone(), cx)?;
      if let Err(e) = write_d_ts(&manifest.dir, &modules) {
        tracing::debug!("types of `{}` not written: {e:#}", manifest.id);
      }
      ScriptView::load(&path, plugin, modules, watches, window, cx)
    };

    let mut failed = None;
    // ponytail: a failed load makes an empty view, gpui needs one to make the entity
    let view = cx.new(|cx| {
      make(window, cx).unwrap_or_else(|e| {
        failed = Some(e);
        ScriptView::new(EMPTY, Vec::new(), Watches::default(), window, cx).expect("empty view")
      })
    });
    if let Some(e) = failed {
      return Err(e);
    }
    let reload = reload_view(&view, window.window_handle(), dir, make, cx);
    let root = cx.new(|_| ScriptRoot::new(view));
    root.update(cx, |root, cx| {
      root.set_background(Some(transparent_black()), cx);
      root.set_fill(matches!(entry, Entry::Panel(_)), cx);
    });
    Ok(Script {
      root,
      opener,
      _reload: reload,
    })
  }

  /// Starts the service of plugin `id`, without a window. Its `corona/surface` opens the
  /// plugin's panels where the shell puts them.
  pub fn start_service(id: &str, cx: &mut App) -> Result<Service> {
    let manifest = Self::manifest(id, cx)?;
    let entry = (manifest.service.as_ref())
      .map(|service| manifest.dir.join(service))
      .with_context(|| format!("plugin `{id}` has no service"))?;
    let dir = manifest.dir.clone();

    let start = move |cx: &mut App| {
      let plugin = Self::plugin(&manifest, cx);
      let watches = Watches::default();
      let load = Load {
        service: true,
        ..Load::default()
      };
      let modules = Self::modules(&manifest, &plugin, &watches, load, cx)?;
      if let Err(e) = write_d_ts(&manifest.dir, &modules) {
        tracing::debug!("types of `{}` not written: {e:#}", manifest.id);
      }
      corona_js::Service::start(&entry, plugin, modules, watches, cx)
    };
    Ok(Service {
      _watch: corona_js::Service::watch(&dir, start, cx),
    })
  }
}

/// Writes the `gpui-kit.d.ts` of a plugin in `dir` with `modules`, when it changed
fn write_d_ts(dir: &Path, modules: &[ModuleBuilder]) -> Result<PathBuf> {
  let path = dir.join(PLUGIN_TYPES_FILENAME);
  let text = format!("{}{STDLIB_D_TS}{}", GpuiKit::d_ts(), d_ts(modules));
  if fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
    fs::write(&path, text).with_context(|| path.display().to_string())?;
  }
  Ok(path)
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
  let hub = ScriptManager::hub(id, cx);
  hub.call(method, args, cx)
}

#[cfg(test)]
mod tests {
  use std::{path::PathBuf, time::Duration};

  use corona_config::Config;
  use corona_js::ScriptExt;
  use corona_utils::test_bus::wait_until;
  use gpui_kit::{self as gpui, TestAppContext, VisualTestContext};

  use super::*;
  use crate::{
    PLUGIN_MANIFEST_FILENAME,
    plugin::registry,
    test_support::{Fake, setup},
  };

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
      let mut manager = ScriptManager::new(self.paths.clone());
      let root = &registry::roots(&self.paths, &Default::default())[1];
      manager.set_plugins(
        registry::scan_root(&self.paths, root, &Default::default())
          .into_iter()
          .map(|found| (found.manifest.id.clone(), found.manifest))
          .collect(),
      );
      manager
    }
  }

  fn manifest(id: &str, extra: &str) -> String {
    format!(
      "id = \"{id}\"\nname = \"Test\"\napi = 2\n{extra}\n[widgets.main]\nview = \"main.js\"\n"
    )
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

  /// `plugins` running, without a window
  fn running(cx: &mut TestAppContext, plugins: &Plugins) {
    cx.set_global(plugins.manager());
    cx.update(|cx| {
      crate::register_modules(cx.script_mut());
      if !cx.has_global::<Config>() {
        cx.set_global(Config::default());
      }
    });
  }

  /// A window to show views in, with `plugins` running
  fn host<'a>(
    cx: &'a mut TestAppContext,
    plugins: &Plugins,
  ) -> (Entity<Host>, &'a mut VisualTestContext) {
    running(cx, plugins);
    window(cx)
  }

  /// A window to show views in
  fn window(cx: &mut TestAppContext) -> (Entity<Host>, &mut VisualTestContext) {
    cx.add_window_view(|_, _| Host(Vec::new()))
  }

  /// Loads view `main` of plugin `id` and shows it in `host`.
  fn show(cx: &mut VisualTestContext, host: &Entity<Host>, id: &str) -> Result<Script> {
    let result =
      cx.update(|window, cx| ScriptManager::load(id, Entry::Widget("main", None), window, cx));
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

  /// Starts the service of plugin `id`; it uses no window.
  fn service(cx: &mut TestAppContext, id: &str) -> Service {
    let service = cx
      .update(|cx| ScriptManager::start_service(id, cx))
      .unwrap();
    cx.run_until_parked();
    service
  }

  #[test]
  fn grants_are_rooted_in_the_plugin_dir() {
    let plugins = Plugins::new();
    let extra = r#"capabilities = { fs = { execute = ["git"] }, corona = ["weather"] }"#;
    plugins.add("a", &manifest("a", extra), &[]);
    let manager = plugins.manager();
    let manifest = &manager.plugins["a"];
    assert!(manifest.capabilities.may_run("git"));
    assert_eq!(manifest.modules, ["weather".to_string()].into());
  }

  #[gpui::test]
  fn load_errors(cx: &mut TestAppContext) {
    let plugins = Plugins::new();
    plugins.add("a", &manifest("a", ""), &[("main.js", VIEW)]);
    cx.set_global(plugins.manager());
    cx.set_global(Config::default());
    let cx = cx.add_empty_window();

    let error = cx
      .update(|window, cx| ScriptManager::load("missing", Entry::Widget("main", None), window, cx))
      .err()
      .unwrap();
    assert!(error.to_string().contains("not found"), "{error}");

    for entry in [Entry::Widget("other", None), Entry::Panel("main")] {
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
    assert!(!plugins.paths.local.join("a/localStorage.json").exists());
  }

  #[gpui::test]
  fn roots_are_transparent_and_widgets_sized_to_content(cx: &mut TestAppContext) {
    let plugins = Plugins::new();
    let extra = "[panels.p]\nview = \"main.js\"";
    plugins.add("a", &manifest("a", extra), &[("main.js", VIEW)]);
    cx.set_global(plugins.manager());
    cx.set_global(Config::default());
    let cx = cx.add_empty_window();
    for (entry, fills) in [
      (Entry::Widget("main", None), false),
      (Entry::Panel("p"), true),
    ] {
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
  fn loads_a_typescript_view(cx: &mut TestAppContext) {
    let fake = setup(cx);
    let plugins = Plugins::new();
    let view = r#"
import { View } from "gpui-kit";
import { v_flex } from "gpui-base";
import { focusWorkspace } from "corona/compositor";
import { greet } from "./util";

export default class Main extends View {
  init(): void { focusWorkspace(greet("ts")); }
  render(_cx: unknown) { return v_flex().child("plugin"); }
}
"#;
    let util = "export const greet = (name: string): string => `hello ${name}`;";
    let manifest =
      manifest("a", "capabilities = { corona = [\"compositor\"] }").replace("main.js", "main.ts");
    plugins.add("a", &manifest, &[("main.ts", view), ("util.ts", util)]);
    let (_, script) = load(cx, &plugins, "a");
    script.unwrap();
    assert_eq!(fake.calls.borrow().last().unwrap(), "hello ts");
  }

  #[gpui::test]
  fn plugins_do_not_share_globals(cx: &mut TestAppContext) {
    let fake = setup(cx);
    let plugins = Plugins::new();
    let view = |init: &str| {
      VIEW
        .replace("render(", &format!("init() {{ {init} }}\n  render("))
        .replace(
          "\"gpui-base\";",
          "\"gpui-base\";\nimport { focusWorkspace } from \"corona/compositor\";",
        )
    };
    let extra = "capabilities = { corona = [\"compositor\"] }";
    let a =
      view("globalThis.leak = 'a'; try { Promise.prototype.then = () => 'patched'; } catch {}");
    let b = view("focusWorkspace(`${globalThis.leak} ${Promise.prototype.then.name}`);");
    plugins.add("a", &manifest("a", extra), &[("main.js", &a)]);
    plugins.add("b", &manifest("b", extra), &[("main.js", &b)]);
    let (host, cx) = host(cx, &plugins);
    let _a = show(cx, &host, "a").unwrap();
    let _b = show(cx, &host, "b").unwrap();
    assert_eq!(fake.calls.borrow().last().unwrap(), "undefined then");
  }

  #[gpui::test]
  fn a_missing_view_fails_the_load(cx: &mut TestAppContext) {
    let plugins = Plugins::new();
    plugins.add("a", &manifest("a", ""), &[]);
    let (_, script) = load(cx, &plugins, "a");
    let error = format!("{:#}", script.err().unwrap());
    assert!(error.contains("main.js"), "{error}");
  }

  #[gpui::test]
  fn unknown_modules_fail_the_load(cx: &mut TestAppContext) {
    let plugins = Plugins::new();
    let extra = r#"capabilities = { corona = ["camera"] }"#;
    plugins.add("a", &manifest("a", extra), &[("main.js", VIEW)]);
    let (_, script) = load(cx, &plugins, "a");
    let error = format!("{:#}", script.err().unwrap());
    assert!(error.contains("unknown module `camera`"), "{error}");
  }

  #[gpui::test]
  fn corona_modules_are_callable_and_rerender(cx: &mut TestAppContext) {
    let fake = setup(cx);
    let plugins = Plugins::new();
    let extra = r#"capabilities = { corona = ["compositor"] }"#;
    plugins.add("a", &manifest("a", extra), &[("main.js", COMPOSITOR_VIEW)]);
    let (cx, script) = load(cx, &plugins, "a");
    let script = script.unwrap();
    let rendered = fake.calls.borrow().len();
    assert!(rendered > 0, "the view rendered");
    assert_eq!(fake.calls.borrow()[0], "rendered 2");

    // the view read the workspaces, so a change renders it again
    let workspaces = cx.update(|_, cx| {
      cx.global::<corona_compositor::Compositor>()
        .workspaces
        .clone()
    });
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
import * as control from "corona/compositor_control";

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
      focusWindow: control.focusWindow("0xa"),
      closeWindow: control.closeWindow("0xb"),
      cursor: control.cursorPosition(),
    };
    c.focusWorkspace(JSON.stringify(results));
    return v_flex().child("plugin");
  }
}
"#;

  #[gpui::test]
  fn calls_every_compositor_function(cx: &mut TestAppContext) {
    let fake = setup(cx);
    let plugins = Plugins::new();
    let extra = r#"capabilities = { corona = ["compositor", "compositor_control"] }"#;
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
    // `undefined` is left out of the JSON
    assert!(results.get("focusWindow").is_none());
    assert!(results.get("closeWindow").is_none());
    assert_eq!(results["cursor"], serde_json::json!({ "x": 3, "y": 4 }));
  }

  #[gpui::test]
  fn ungranted_corona_modules_cannot_be_imported(cx: &mut TestAppContext) {
    let fake = setup(cx);
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
    let fake = setup(cx);
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
    const thrown = (f) => { try { f(); } catch (e) { return e.message; } };
    const errors = [togglePanel, openPanel, closePanel, isPanelOpen]
      .filter((f) => thrown(() => f("other"))?.includes("no panel `other`")).length;
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
    let fake = setup(cx);
    cx.update(|cx| corona_surface::init(cx).unwrap());
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
  fn widgets_read_their_options(cx: &mut TestAppContext) {
    let fake = setup(cx);
    let plugins = Plugins::new();
    let view = r#"
import { View } from "gpui-kit";
import { v_flex } from "gpui-base";
import { focusWorkspace } from "corona/compositor";
import { options } from "corona/surface";
export default class Main extends View {
  render() {
    focusWorkspace("options " + JSON.stringify(options()));
    return v_flex().child("plugin");
  }
}
"#;
    let extra = "capabilities = { corona = [\"compositor\"] }\n[panels.p]\nview = \"main.js\"";
    plugins.add("a", &manifest("a", extra), &[("main.js", view)]);
    let (host, cx) = host(cx, &plugins);
    let options = serde_json::json!({ "city": "Rosenheim" });
    for entry in [Entry::Widget("main", Some(&options)), Entry::Panel("p")] {
      let script = cx.update(|window, cx| ScriptManager::load("a", entry, window, cx));
      let view = script.unwrap().view();
      cx.update(|_, cx| {
        host.update(cx, |host, cx| {
          host.0.push(view);
          cx.notify();
        })
      });
      cx.run_until_parked();
    }
    let mut options = reported(&fake, "options ");
    options.dedup();
    assert_eq!(options, [r#"{"city":"Rosenheim"}"#, "null"]);
  }

  #[gpui::test]
  fn services_hear_of_setting_and_secret_changes(cx: &mut TestAppContext) {
    let fake = setup(cx);
    let plugins = Plugins::new();
    let source = r#"
import { focusWorkspace } from "corona/compositor";
import { onChange } from "corona/settings";
export default function main() {
  onChange((change) => focusWorkspace("change " + JSON.stringify(change)));
}
"#;
    let extra = r#"capabilities = { corona = ["compositor", "secrets"] }
service = "service.js"
[[settings]]
key = "label"
label = "Label"
type = "text"
default = "hello"
[[settings]]
key = "count"
label = "Count"
type = "number"
default = 1
[[settings]]
key = "token"
label = "Token"
type = "secret""#;
    plugins.add(
      "a",
      &manifest("a", extra),
      &[("main.js", VIEW), ("service.js", source)],
    );
    running(cx, &plugins);
    let _service = service(cx, "a");

    // only what changed, and a secret's key without its value
    cx.update(|cx| {
      let set = serde_json::json!({ "label": "bye", "count": 1, "token": "x" });
      (cx.global_mut::<Config>().plugin_settings)
        .insert("a".into(), set.as_object().unwrap().clone());
      ScriptManager::secret_changed("a", "token", cx);
      // another plugin's secret is not this one's
      ScriptManager::secret_changed("b", "token", cx);
    });
    cx.run_until_parked();
    // the config tells its observers once the update is done, after the secret
    let mut changes = reported(&fake, "change ");
    changes.sort();
    assert_eq!(
      changes,
      [
        r#"{"key":"label","value":"bye"}"#,
        r#"{"key":"token","secret":true}"#
      ]
    );
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

  fn reported(fake: &Fake, prefix: &str) -> Vec<String> {
    let calls = fake.calls.borrow();
    let found = calls.iter().filter_map(|c| c.strip_prefix(prefix));
    found.map(str::to_string).collect()
  }

  /// Echoes `echo`, never answers `hang`, fails everything else.
  const SERVICE: &str = r#"
import { onCall, reply, fail } from "corona/plugin";

export default function main(_cx) {
  onCall((call) => {
    if (call.method === "echo") reply(call.id, call.args);
    else if (call.method !== "hang") fail(call.id, "no " + call.method);
  });
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
      let answer;
      try {
        answer = await call("METHOD", 1);
      } catch (e) {
        answer = { message: e.message };
      }
      focusWorkspace("answer " + JSON.stringify(answer));
    });
  }
  render() { return v_flex().child("plugin"); }
}
"#;

  fn caller(plugins: &Plugins, method: &str, service: bool) {
    let extra = match service {
      true => "capabilities = { corona = [\"compositor\"] }\nservice = \"service.js\"",
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
    let fake = setup(cx);
    let plugins = Plugins::new();
    caller(&plugins, "echo", true);
    running(cx, &plugins);
    let _service = service(cx, "a");
    assert!(cx.windows().is_empty());
    let (host, cx) = window(cx);
    let _view = show(cx, &host, "a").unwrap();
    assert_eq!(reported(&fake, "answer "), ["1"]);
  }

  /// Reads `PATH` before and after picking it, and reports both.
  const PICKER: &str = r#"
import { View } from "gpui-kit";
import { v_flex } from "gpui-base";
import { focusWorkspace } from "corona/compositor";
import { pickFiles } from "corona/desktop";
import { readFile } from "fs/promises";

async function read(path) {
  try {
    return "read " + await readFile(path, "utf8");
  } catch (e) {
    return "denied " + e;
  }
}

export default class Main extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      focusWorkspace("before " + await read("PATH"));
      const [picked] = await pickFiles();
      focusWorkspace("after " + await read(picked));
    });
  }
  render() { return v_flex().child("plugin"); }
}
"#;

  #[gpui::test]
  fn picked_files_become_readable(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let fake = setup(cx);
    let plugins = Plugins::new();
    let outside = tempfile::tempdir().unwrap();
    let file = outside.path().join("picked.txt");
    fs::write(&file, "hi").unwrap();
    let path = file.to_string_lossy();
    let extra = r#"capabilities = { corona = ["compositor", "desktop"] }"#;
    plugins.add(
      "a",
      &manifest("a", extra),
      &[("main.js", &PICKER.replace("PATH", &path))],
    );
    let (cx, script) = load(cx, &plugins, "a");
    let _script = script.unwrap();
    wait_until(cx, |cx| cx.did_prompt_for_paths());
    assert!(reported(&fake, "before ")[0].starts_with("denied"));
    cx.simulate_path_prompt_response(|_| Some(vec![file.clone()]));
    wait_until(cx, |_| !reported(&fake, "after ").is_empty());
    assert_eq!(reported(&fake, "after "), ["read hi"]);
  }

  #[gpui::test]
  fn the_service_fails_what_it_does_not_know(cx: &mut TestAppContext) {
    let fake = setup(cx);
    let plugins = Plugins::new();
    caller(&plugins, "other", true);
    running(cx, &plugins);
    let _service = service(cx, "a");
    let (host, cx) = window(cx);
    let _view = show(cx, &host, "a").unwrap();
    assert_eq!(reported(&fake, "answer "), [r#"{"message":"no other"}"#]);
  }

  #[gpui::test]
  fn calls_without_a_service_fail(cx: &mut TestAppContext) {
    let fake = setup(cx);
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
      .update(|_, cx| ScriptManager::start_service("a", cx))
      .err()
      .unwrap();
    assert!(error.to_string().contains("has no service"), "{error}");
  }

  #[gpui::test]
  fn calls_time_out(cx: &mut TestAppContext) {
    let fake = setup(cx);
    let plugins = Plugins::new();
    caller(&plugins, "hang", true);
    running(cx, &plugins);
    let _service = service(cx, "a");
    let (host, cx) = window(cx);
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
    let fake = setup(cx);
    let plugins = Plugins::new();
    caller(&plugins, "hang", true);
    running(cx, &plugins);
    let service = service(cx, "a");
    let (host, cx) = window(cx);
    let _view = show(cx, &host, "a").unwrap();
    assert!(reported(&fake, "answer ").is_empty());
    drop(service);
    cx.run_until_parked();
    assert_eq!(
      reported(&fake, "answer "),
      [r#"{"message":"service stopped"}"#]
    );
  }

  /// What the service answers `method` with, through `corona ipc`'s path
  fn ipc(cx: &mut TestAppContext, method: &str) -> Result<serde_json::Value> {
    let answer = cx.update(|cx| crate::call("a", method.into(), "hi".into(), cx))?;
    let answer = cx.executor().spawn(answer);
    cx.run_until_parked();
    futures_lite::future::block_on(answer)
  }

  #[gpui::test]
  fn ipc_calls_reach_the_service(cx: &mut TestAppContext) {
    setup(cx);
    let plugins = Plugins::new();
    caller(&plugins, "echo", true);
    running(cx, &plugins);
    let _service = service(cx, "a");
    assert_eq!(ipc(cx, "echo").unwrap(), serde_json::json!("hi"));
    let missing = cx.update(|cx| crate::call("b", "echo".into(), "hi".into(), cx).err());
    assert!(missing.unwrap().to_string().contains("not found"));
    assert!(cx.windows().is_empty());
  }

  /// Answers with its version and reports it on a timer.
  const VERSIONED_SERVICE: &str = r#"
import { onCall, reply } from "corona/plugin";
import { focusWorkspace } from "corona/compositor";

export default function main(cx) {
  cx.timer.every(10, () => focusWorkspace("tick VERSION"));
  onCall((call) => reply(call.id, "VERSION"));
}
"#;

  #[gpui::test]
  fn an_edited_service_restarts(cx: &mut TestAppContext) {
    let fake = setup(cx);
    let plugins = Plugins::new();
    caller(&plugins, "echo", true);
    let path = plugins.paths.local.join("a/service.js");
    fs::write(&path, VERSIONED_SERVICE.replace("VERSION", "old")).unwrap();
    running(cx, &plugins);
    let _service = service(cx, "a");
    assert_eq!(ipc(cx, "echo").unwrap(), serde_json::json!("old"));

    // older than the write, so the stamp moves
    std::thread::sleep(Duration::from_millis(20));
    fs::write(&path, VERSIONED_SERVICE.replace("VERSION", "new")).unwrap();
    // the poll runs on the executor's clock, the debounce on the wall's
    for _ in 0..2 {
      cx.executor().advance_clock(Duration::from_millis(500));
      cx.run_until_parked();
      std::thread::sleep(Duration::from_millis(250));
    }
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    assert_eq!(ipc(cx, "echo").unwrap(), serde_json::json!("new"));

    // the old one is gone
    let old = reported(&fake, "tick ")
      .iter()
      .filter(|t| *t == "old")
      .count();
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.run_until_parked();
    let ticks = reported(&fake, "tick ");
    assert_eq!(ticks.iter().filter(|t| *t == "old").count(), old);
    assert!(ticks.iter().any(|t| t == "new"));
  }

  /// Counts its starts in the shared state and reports what it reads.
  const STATE_VIEW: &str = r#"
import { View } from "gpui-kit";
import { v_flex } from "gpui-base";
import { focusWorkspace } from "corona/compositor";
import { getState, setState } from "corona/plugin";

export default class Main extends View {
  init(_props, cx) {
    setState("starts", (getState("starts") ?? 0) + 1);
  }
  render() {
    focusWorkspace("starts " + getState("starts"));
    return v_flex().child("plugin");
  }
}
"#;

  #[gpui::test]
  fn state_is_shared_and_rerenders(cx: &mut TestAppContext) {
    let fake = setup(cx);
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

    // so does the service, which sets it too
    fs::write(
      plugins.paths.local.join("a/service.js"),
      r#"import { getState, setState } from "corona/plugin";
export default function main() { setState("starts", getState("starts") * 10); }"#,
    )
    .unwrap();
    let manifest = manifest(
      "a",
      "capabilities = { corona = [\"compositor\"] }\nservice = \"service.js\"",
    );
    fs::write(plugins.paths.local.join("a/plugin.toml"), manifest).unwrap();
    let running = plugins.manager().plugins().clone();
    cx.update(|_, cx| cx.global_mut::<ScriptManager>().set_plugins(running));
    let _service = cx
      .update(|_, cx| ScriptManager::start_service("a", cx))
      .unwrap();
    cx.run_until_parked();
    assert_eq!(reported(&fake, "starts ").last().unwrap(), "20");
  }

  #[gpui::test]
  fn only_the_service_takes_calls(cx: &mut TestAppContext) {
    let fake = setup(cx);
    let plugins = Plugins::new();
    let view = r#"
import { View } from "gpui-kit";
import { onCall } from "corona/plugin";
export default class Main extends View { render() { return onCall; } }
"#;
    // and only views call it
    let main = r#"
import { focusWorkspace } from "corona/compositor";
import * as plugin from "corona/plugin";
export default function main() {
  focusWorkspace(["call", "onCall", "reply", "fail"].filter((f) => f in plugin).join());
}
"#;
    let extra = "capabilities = { corona = [\"compositor\"] }\nservice = \"service.js\"";
    plugins.add(
      "a",
      &manifest("a", extra),
      &[("main.js", view), ("service.js", main)],
    );
    running(cx, &plugins);
    let _service = service(cx, "a");
    assert_eq!(fake.calls.borrow().last().unwrap(), "onCall,reply,fail");
    let (host, cx) = window(cx);
    let error = format!(
      "{:#}",
      show(cx, &host, "a").err().expect("the import fails")
    );
    assert!(error.contains("onCall"), "{error}");
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
    let fake = setup(cx);
    let plugins = Plugins::new();
    let extra = r#"capabilities = { corona = ["compositor"] }"#;
    plugins.add("a", &manifest("a", extra), &[("main.js", START_COUNTER)]);
    let (_, cx) = host(cx, &plugins);
    let start = |cx: &mut VisualTestContext| {
      let script =
        cx.update(|window, cx| ScriptManager::load("a", Entry::Widget("main", None), window, cx));
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

  /// Reports translations.
  const I18N_VIEW: &str = r#"
import { View } from "gpui-kit";
import { v_flex } from "gpui-base";
import { focusWorkspace } from "corona/compositor";
import { t, language } from "corona/i18n";

export default class Main extends View {
  render() {
    focusWorkspace([language(), t("greet", { name: "x" }), t("only.en"), t("missing")].join("|"));
    return v_flex().child("plugin");
  }
}
"#;

  #[gpui::test]
  fn translations_fall_back_and_rerender(cx: &mut TestAppContext) {
    let fake = setup(cx);
    let plugins = Plugins::new();
    let extra = r#"capabilities = { corona = ["compositor"] }"#;
    fs::create_dir_all(plugins.paths.local.join("a/locales")).unwrap();
    plugins.add(
      "a",
      &manifest("a", extra),
      &[
        ("main.js", I18N_VIEW),
        (
          "locales/app.yml",
          "_version: 2\ngreet:\n  en: hi %{name}\n  de: hallo %{name}\nonly:\n  en:\n    en: english\n",
        ),
      ],
    );
    rust_i18n::set_locale("en");
    let (cx, script) = load(cx, &plugins, "a");
    let _script = script.unwrap();
    assert_eq!(
      fake.calls.borrow().last().unwrap(),
      "en|hi x|english|missing"
    );

    rust_i18n::set_locale("de-AT");
    cx.update(|_, cx| cx.global_mut::<Config>().shell.language = Some("de".into()));
    cx.run_until_parked();
    assert_eq!(
      fake.calls.borrow().last().unwrap(),
      "de-AT|hallo x|english|missing"
    );
  }

  #[gpui::test]
  fn every_plugin_has_plugin_and_i18n(cx: &mut TestAppContext) {
    let plugins = Plugins::new();
    plugins.add("a", &manifest("a", ""), &[("main.js", VIEW)]);
    let (_, script) = load(cx, &plugins, "a");
    script.unwrap();
    let dts = || fs::read_to_string(plugins.paths.local.join("a/gpui-kit.d.ts")).unwrap();
    let (call, next) = (
      "  export function call(method: string, args?: Value | null): Promise<Value>;",
      "  export function onCall(cb: (value: Call) => void): () => void;",
    );
    let view = dts();
    for line in [
      "declare module \"corona/plugin\"",
      call,
      next,
      "declare module \"corona/i18n\"",
    ] {
      assert!(view.contains(line), "missing {line:?} in\n{view}");
    }

    // the service's are the same, or the editor's flip with what ran last
    plugins.add(
      "a",
      &manifest("a", "service = \"service.js\""),
      &[("service.js", "export default function main() {}")],
    );
    cx.set_global(plugins.manager());
    let _service = service(cx, "a");
    assert_eq!(dts(), view);
  }

  /// Reports through `focusWorkspace` whether the test name is on the bus,
  /// and what calling an ungranted one answers.
  const DBUS_VIEW: &str = r#"
import { View } from "gpui-kit";
import { v_flex } from "gpui-base";
import { focusWorkspace } from "corona/compositor";
import { call, nameHasOwner } from "corona/dbus";

let started = false;

export default class Main extends View {
  render(_cx) {
    if (!started) {
      started = true;
      nameHasOwner("session", "io.corona.Test").then((owned) => focusWorkspace("owned " + owned));
      call({ bus: "session", dest: "io.corona.Other", path: "/", iface: "io.corona.Other", method: "X" })
        .catch((e) => focusWorkspace("denied " + e.message));
    }
    return v_flex().child("plugin");
  }
}
"#;

  #[gpui::test]
  fn dbus_calls_go_to_granted_names(cx: &mut TestAppContext) {
    use corona_utils::test_bus::{TestBus, wait_until};
    use futures_lite::future::block_on;

    cx.executor().allow_parking();
    let bus = TestBus::new();
    let (session, service) = block_on(async { (bus.conn().await, bus.conn().await) });
    block_on(service.request_name("io.corona.Test")).unwrap();
    cx.set_global(crate::Buses {
      system: session.clone(),
      session,
    });
    let fake = setup(cx);
    let plugins = Plugins::new();
    let extra =
      r#"capabilities = { corona = ["compositor"], dbus = { session = ["io.corona.Test"] } }"#;
    plugins.add("a", &manifest("a", extra), &[("main.js", DBUS_VIEW)]);
    let (cx, script) = load(cx, &plugins, "a");
    let _script = script.unwrap();

    wait_until(cx, |_| fake.calls.borrow().len() == 2);
    let calls = fake.calls.borrow();
    assert!(calls.contains(&"owned true".to_string()), "{calls:?}");
    assert!(
      calls
        .iter()
        .any(|c| c.starts_with("denied ") && c.contains("not in this plugin's session bus grant")),
      "{calls:?}"
    );
  }

  /// Imports one module and does nothing with it.
  fn importing(module: &str) -> String {
    format!(
      r#"
import {{ View }} from "gpui-kit";
import {{ v_flex }} from "gpui-base";
import * as m from "corona/{module}";

export default class Main extends View {{
  render(_cx) {{
    return v_flex().child(typeof m);
  }}
}}
"#
    )
  }

  #[gpui::test]
  fn modules_need_their_grant(cx: &mut TestAppContext) {
    let plugins = Plugins::new();
    for (id, module, extra, granted) in [
      ("a", "dbus", "", false),
      ("b", "dbus", r#"capabilities = { dbus = {} }"#, true),
      ("c", "secrets", "", false),
      (
        "d",
        "secrets",
        r#"capabilities = { corona = ["secrets"] }"#,
        true,
      ),
      ("e", "desktop", "", false),
      (
        "f",
        "desktop",
        r#"capabilities = { corona = ["desktop"] }"#,
        true,
      ),
    ] {
      plugins.add(id, &manifest(id, extra), &[("main.js", &importing(module))]);
      let (_, script) = load(cx, &plugins, id);
      match script {
        Ok(_) => assert!(granted, "{module} imported without a grant"),
        Err(error) => {
          assert!(!granted, "{module}: {error:#}");
          assert!(
            format!("{error:#}").contains(&format!("corona/{module}")),
            "{error:#}"
          );
        }
      }
    }
  }

  /// Reports what the user picked through `focusWorkspace`.
  const PICK_VIEW: &str = r#"
import { View } from "gpui-kit";
import { v_flex } from "gpui-base";
import { focusWorkspace } from "corona/compositor";
import { pickFiles } from "corona/desktop";

let started = false;

export default class Main extends View {
  render(_cx) {
    if (!started) {
      started = true;
      pickFiles({ multiple: true, acceptLabel: "Send" }).then((paths) => focusWorkspace(JSON.stringify(paths)));
    }
    return v_flex().child("plugin");
  }
}
"#;

  #[gpui::test]
  fn picks_files(cx: &mut TestAppContext) {
    let fake = setup(cx);
    let plugins = Plugins::new();
    let extra = r#"capabilities = { corona = ["compositor", "desktop"] }"#;
    plugins.add("a", &manifest("a", extra), &[("main.js", PICK_VIEW)]);
    let (cx, script) = load(cx, &plugins, "a");
    let _script = script.unwrap();

    cx.simulate_path_prompt_response(|options| {
      assert!(options.multiple && options.files && !options.directories);
      assert_eq!(options.prompt.as_deref(), Some("Send"));
      Some(vec!["/tmp/a".into(), "/tmp/b".into()])
    });
    cx.run_until_parked();
    assert_eq!(
      fake.calls.borrow().last().unwrap(),
      r#"["/tmp/a","/tmp/b"]"#
    );
  }

  /// Every corona module's name in `capabilities.corona`
  const ALL_MODULES: [&str; 18] = [
    "compositor",
    "compositor_control",
    "pipewire",
    "network",
    "mpris",
    "bluetooth",
    "power",
    "brightness",
    "notifications",
    "notification_center",
    "tray",
    "sysinfo",
    "weather",
    "auth",
    "secrets",
    "desktop",
    "idle",
    "printing",
  ];

  fn dts(plugins: &Plugins, id: &str) -> String {
    fs::read_to_string(plugins.paths.local.join(id).join("gpui-kit.d.ts")).unwrap()
  }

  #[gpui::test]
  fn types_need_no_running_services(cx: &mut TestAppContext) {
    let plugins = Plugins::new();
    let modules = ALL_MODULES.map(|m| format!("\"{m}\"")).join(", ");
    let extra = format!(
      "capabilities = {{ corona = [{modules}], dbus = {{ session = [\"io.corona.Test\"] }} }}"
    );
    plugins.add("a", &manifest("a", &extra), &[("main.js", VIEW)]);
    let dir = plugins.paths.local.join("a");

    // no compositor, pipewire, … nor a script manager or config
    let written = cx
      .update(|cx| crate::write_plugin_types(std::slice::from_ref(&dir), cx))
      .unwrap();

    assert!(written.contains(&dir.join("gpui-kit.d.ts")), "{written:?}");
    let dts = dts(&plugins, "a");
    for module in ALL_MODULES
      .iter()
      .chain(&["dbus", "settings", "plugin", "i18n", "surface"])
    {
      let line = format!("declare module \"corona/{module}\"");
      assert!(dts.contains(&line), "missing {line:?}");
    }
    let registered = cx.update(|cx| cx.global::<ScriptRegistry>().all().count());
    assert_eq!(registered, ALL_MODULES.len());
  }

  #[gpui::test]
  fn types_only_declare_granted_modules(cx: &mut TestAppContext) {
    let plugins = Plugins::new();
    let extra = r#"capabilities = { corona = ["compositor"] }"#;
    plugins.add("a", &manifest("a", extra), &[("main.js", VIEW)]);
    running(cx, &plugins);
    let dir = plugins.paths.local.join("a");
    cx.update(|cx| ScriptManager::write_types(&dir, cx))
      .unwrap();

    let dts = dts(&plugins, "a");
    assert!(dts.contains("declare module \"corona/compositor\""));
    for module in ["pipewire", "dbus", "weather"] {
      let line = format!("declare module \"corona/{module}\"");
      assert!(!dts.contains(&line), "ungranted {line:?}");
    }
  }

  #[gpui::test]
  fn types_type_the_settings(cx: &mut TestAppContext) {
    let plugins = Plugins::new();
    let extra = "[[settings]]\nkey = \"units\"\nlabel = \"Units\"\ntype = \"select\"\ndefault = \"a\"\noptions = [{ value = \"a\", label = \"A\" }, { value = \"b\", label = \"B\" }]";
    plugins.add("a", &manifest("a", extra), &[("main.js", VIEW)]);
    running(cx, &plugins);
    let dir = plugins.paths.local.join("a");
    cx.update(|cx| ScriptManager::write_types(&dir, cx))
      .unwrap();

    let dts = dts(&plugins, "a");
    let start = dts.find("declare module \"corona/settings\"").unwrap();
    let module = &dts[start..start + dts[start..].find("\n}").unwrap()];
    assert!(module.contains("    \"units\": \"a\" | \"b\";"), "{module}");
    // and leave no hub behind for a plugin that never ran
    assert!(cx.update(|cx| !cx.global::<ScriptManager>().hubs.contains_key("a")));
  }

  #[gpui::test]
  fn types_match_a_load(cx: &mut TestAppContext) {
    setup(cx);
    let plugins = Plugins::new();
    let extra = "service = \"service.js\"\ncapabilities = { corona = [\"compositor\"] }\n[[settings]]\nkey = \"label\"\nlabel = \"Label\"\ntype = \"text\"\ndefault = \"\"";
    plugins.add(
      "a",
      &manifest("a", extra),
      &[
        ("main.js", COMPOSITOR_VIEW),
        ("service.js", "export default function main() {}"),
      ],
    );
    let (cx, script) = load(cx, &plugins, "a");
    script.unwrap();
    let loaded = dts(&plugins, "a");

    let path = plugins.paths.local.join("a/gpui-kit.d.ts");
    fs::remove_file(&path).unwrap();
    let dir = plugins.paths.local.join("a");
    cx.update(|_, cx| ScriptManager::write_types(&dir, cx))
      .unwrap();
    assert_eq!(dts(&plugins, "a"), loaded);
    // a running plugin keeps its hub
    assert!(cx.update(|_, cx| cx.global::<ScriptManager>().hubs.contains_key("a")));
  }

  #[gpui::test]
  fn types_keep_a_tsconfig(cx: &mut TestAppContext) {
    let plugins = Plugins::new();
    let tsconfig = "{ \"extends\": \"../tsconfig.json\" }\n";
    plugins.add(
      "a",
      &manifest("a", ""),
      &[("main.js", VIEW), ("tsconfig.json", tsconfig)],
    );
    running(cx, &plugins);
    let dir = plugins.paths.local.join("a");
    cx.update(|cx| ScriptManager::write_types(&dir, cx))
      .unwrap();

    assert!(dir.join("gpui-kit.d.ts").is_file());
    assert!(!dir.join("jsconfig.json").exists());
    assert_eq!(
      fs::read_to_string(dir.join("tsconfig.json")).unwrap(),
      tsconfig
    );
  }

  #[gpui::test]
  fn types_fail_without_a_manifest(cx: &mut TestAppContext) {
    let plugins = Plugins::new();
    running(cx, &plugins);
    let dir = plugins.paths.local.join("none");
    assert!(
      cx.update(|cx| ScriptManager::write_types(&dir, cx))
        .is_err()
    );
    assert!(!dir.join("gpui-kit.d.ts").exists());
  }
}
