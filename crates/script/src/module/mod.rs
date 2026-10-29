//! The `corona/*` modules: the ones plugins are granted by name in
//! `capabilities.corona`, registered as [`ScriptModule`](corona_js::ScriptModule)s, and the
//! ones every plugin has (`settings`, `surface`, `plugin`, `i18n`, `dbus` with its grant).

use corona_js::{ScriptRegistry, json};

pub mod auth;
pub mod bluetooth;
pub mod brightness;
pub mod compositor;
pub mod dbus;
pub mod desktop;
pub mod i18n;
pub mod idle;
pub mod mpris;
pub mod network;
pub mod notifications;
pub mod pipewire;
pub mod plugin;
pub mod power;
pub mod secrets;
pub mod settings;
pub mod surface;

/// Registers the modules plugins are granted by name.
pub fn register(registry: &mut ScriptRegistry) {
  registry
    .register::<auth::Auth>()
    .register::<bluetooth::Bluetooth>()
    .register::<brightness::Brightness>()
    .register::<compositor::Compositor>()
    .register::<compositor::CompositorControl>()
    .register::<desktop::Desktop>()
    .register::<idle::Idle>()
    .register::<mpris::Mpris>()
    .register::<network::Network>()
    .register::<notifications::Notifications>()
    .register::<notifications::NotificationCenter>()
    .register::<pipewire::Pipewire>()
    .register::<power::Power>()
    .register::<secrets::Secrets>();
}

/// Runs a module in a real script view, for tests of what plugins see.
#[cfg(test)]
pub(crate) mod harness {
  use std::{fs, rc::Rc};

  use corona_js::{
    Cx, Json, ModuleBuilder, PluginRef, ScriptModule, ScriptView, Watches, host_module,
  };
  use gpui_kit::{App, Entity, Global, TestAppContext, VisualTestContext};
  use serde_json::Value;
  use specta::specta;

  /// What `report(value)` was called with, in order
  #[derive(Default)]
  struct Reports(Vec<Value>);

  impl Global for Reports {}

  /// Hands `value` to the test.
  #[specta]
  fn report(mut cx: Cx, value: Json<Value>) {
    cx.default_global::<Reports>().0.push(value.0);
  }

  pub(crate) struct View {
    _view: Entity<ScriptView>,
    _dir: tempfile::TempDir,
  }

  impl View {
    /// What the script reported, in order
    pub fn reports(&self, cx: &mut TestAppContext) -> Vec<Value> {
      cx.update(|cx| cx.default_global::<Reports>().0.clone())
    }

    pub fn last(&self, cx: &mut TestAppContext) -> Value {
      self.reports(cx).last().cloned().expect("nothing reported")
    }
  }

  /// A view of plugin `p` running `body` on every render, with module `name` that
  /// `module` builds imported as `m`, `report` from `corona/test`, and `attempt(f)`, which
  /// is what `f()` returns, or `{ message }` when it throws.
  pub(crate) fn view<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
    body: &str,
    module: impl FnOnce(&PluginRef, &mut ModuleBuilder, &mut App) + 'static,
  ) -> (View, &'a mut VisualTestContext) {
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("main.js");
    let source = format!(
      r#"
import {{ View, div }} from "gpui-kit";
import {{ report }} from "corona/test";
import * as m from "{name}";

const attempt = (f) => {{
  try {{
    return f() ?? null;
  }} catch (e) {{
    return {{ message: e.message }};
  }}
}};

export default class Main extends View {{
  render(_cx) {{
    {body}
    return div().child("test");
  }}
}}
"#
    );
    fs::write(&main, source).unwrap();
    let plugin = Rc::new(PluginRef {
      id: "p".into(),
      name: "P".into(),
      dir: dir.path().to_path_buf(),
      data_dir: dir.path().join("data"),
      capabilities: Default::default(),
    });
    let name = name.to_string();
    let (view, cx) = cx.add_window_view(move |window, cx| {
      let watches = Watches::default();
      let mut test = ModuleBuilder::new("corona/test", &watches);
      host_module!(&mut test, { sync: [report] });
      let mut m = ModuleBuilder::new(name, &watches);
      module(&plugin, &mut m, cx);
      ScriptView::load(&main, plugin, vec![test, m], watches, window, cx).unwrap()
    });
    cx.run_until_parked();
    (
      View {
        _view: view,
        _dir: dir,
      },
      cx,
    )
  }

  /// [`view`] of the registered module `M`
  pub(crate) fn module<'a, M: ScriptModule>(
    cx: &'a mut TestAppContext,
    body: &str,
  ) -> (View, &'a mut VisualTestContext) {
    view(cx, &format!("corona/{}", M::NAME), body, M::module)
  }
}
