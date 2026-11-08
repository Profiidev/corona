//! The plugins as the shell runs them: their services, bar widgets and panels, and asking
//! the user to approve them.

use std::{
  collections::{HashMap, HashSet},
  rc::Rc,
};

use corona_surface::{
  bar::{BarExt, BarState, WidgetData},
  panel::{PanelData, PanelState},
};
use gpui_kit::{
  App, AppContext, IntoElement, ParentElement, Render, Styled, Window, div, prelude::FluentBuilder,
};
use rust_i18n::t;

use crate::{
  Entry, PluginManager, Script, ScriptManager, Service,
  hooks::{self, Notice},
  plugin::manifest::PluginManifest,
};

/// What is registered for the plugins that run, by id
type Registered = HashMap<String, Registration>;

/// The widget and panel names registered for a plugin, and its service
struct Registration {
  /// The plugin's [`PluginManager::signatures`] entry it was registered for
  signature: String,
  widgets: Vec<String>,
  panels: Vec<String>,
  _service: Option<Service>,
}

pub(crate) fn init(cx: &mut App) {
  cx.set_global(corona_js::TripHook(Rc::new(limit_hit)));
  watch_approvals(cx);
  let mut registered = Registered::new();
  sync(&mut registered, cx);
  let mut revision = cx.global::<PluginManager>().revision();
  cx.observe_global::<PluginManager>(move |cx| {
    let now = cx.global::<PluginManager>().revision();
    if now != revision {
      revision = now;
      // the bar only changes with the widgets
      if sync(&mut registered, cx) {
        BarState::reopen(cx);
      }
    }
  })
  .detach();
}

/// Tells of plugin updates, and asks about each plugin as it starts waiting for approval:
/// in the settings window when open, else with a notification
fn watch_approvals(cx: &mut App) {
  let mut waiting = HashSet::new();
  // the running plugins' versions, to tell of updates
  let mut versions = Versions::new();
  let mut check = move |cx: &mut App| {
    let manager = cx.global::<PluginManager>();
    let now: HashSet<String> = manager.unapproved().keys().cloned().collect();
    let running: Versions = manager
      .active()
      .iter()
      .map(|(id, found)| {
        let m = &found.manifest;
        (id.clone(), (m.name.clone(), m.version.clone()))
      })
      .collect();
    for (name, version) in updates(&mut versions, running, &now) {
      let notice = Notice {
        summary: t!("app.settings.plugins.updated.title", name = name).into(),
        body: t!("app.settings.plugins.updated.body", version = version).into(),
        action: None,
      };
      hooks::notify(notice, cx);
    }
    for id in now.difference(&waiting) {
      if !crate::settings::review_plugin(id, cx) {
        ask(id, cx);
      }
    }
    waiting = now;
  };
  check(cx);
  cx.observe_global::<PluginManager>(check).detach();
}

/// Name and version, by plugin id
type Versions = HashMap<String, (String, Option<String>)>;

/// The `running` plugins whose version differs from the one in `versions`, as
/// name and new version; keeps `versions` up to date. Ones `waiting` for
/// approval keep their last running version, so approving an update tells of
/// it too.
fn updates(
  versions: &mut Versions,
  running: Versions,
  waiting: &HashSet<String>,
) -> Vec<(String, String)> {
  let updated = running
    .iter()
    .filter_map(|(id, (name, version))| {
      let (old, new) = (&versions.get(id)?.1, version.clone()?);
      (old.as_ref() != Some(&new)).then(|| (name.clone(), new))
    })
    .collect();
  versions.retain(|id, _| waiting.contains(id));
  versions.extend(running);
  updated
}

/// Warns that plugin `id` hit a resource limit; it restarts, or is disabled when it keeps
/// hitting one
fn limit_hit(id: &str, trip: corona_js::Trip, give_up: bool, cx: &mut App) {
  let name = (cx.global::<PluginManager>().active().get(id))
    .map_or_else(|| id.to_string(), |p| p.manifest.name.clone());
  let body = match trip {
    corona_js::Trip::Time => t!("app.settings.plugins.limit.time"),
    corona_js::Trip::Memory => t!("app.settings.plugins.limit.memory"),
  };
  let summary = match give_up {
    true => t!("app.settings.plugins.limit.disabled", name = name),
    false => t!("app.settings.plugins.limit.title", name = name),
  };
  let notice = Notice {
    summary: summary.into(),
    body: body.into(),
    action: None,
  };
  hooks::notify(notice, cx);
  if give_up {
    PluginManager::disable(id, cx);
  }
}

/// Asks to review plugin `id` with a notification
fn ask(id: &str, cx: &mut App) {
  let name = cx.global::<PluginManager>().unapproved()[id]
    .manifest
    .name
    .clone();
  let id = id.to_string();
  let review = move |cx: &mut App| {
    if let Err(e) = corona_settings::open(Some("plugins"), cx) {
      tracing::error!("opening the settings: {e:#}");
    }
    crate::settings::review_plugin(&id, cx);
  };
  let notice = Notice {
    summary: t!("app.settings.plugins.approve.changed_title", name = name).into(),
    body: t!("app.settings.plugins.approve.changed_body").into(),
    action: Some((
      t!("app.settings.plugins.approve.review").into(),
      Rc::new(review),
    )),
  };
  hooks::notify(notice, cx);
}

/// Registers what changed plugins need and drops what plugins that stopped or changed had.
/// Whether the bar's widgets changed.
fn sync(registered: &mut Registered, cx: &mut App) -> bool {
  let now = cx.global::<PluginManager>().signatures().clone();
  let mut widgets_changed = false;
  registered.retain(|id, old| {
    if now.get(id) == Some(&old.signature) {
      return true;
    }
    widgets_changed |= !old.widgets.is_empty();
    for name in &old.widgets {
      cx.bar_mut().unregister(name);
    }
    for name in &old.panels {
      PanelState::unregister(name, cx);
    }
    // dropping `old` stops its service, failing what waits for it
    false
  });

  let plugins: Vec<_> = cx
    .global::<PluginManager>()
    .active()
    .values()
    .filter(|found| !registered.contains_key(&found.manifest.id))
    .map(|found| found.manifest.clone())
    .collect();
  for manifest in plugins {
    let registration = register(&manifest, now[&manifest.id].clone(), cx);
    widgets_changed |= !registration.widgets.is_empty();
    registered.insert(manifest.id, registration);
  }
  widgets_changed
}

fn register(manifest: &PluginManifest, signature: String, cx: &mut App) -> Registration {
  let mut registered = Registration {
    signature,
    widgets: Vec::new(),
    panels: Vec::new(),
    _service: None,
  };
  if manifest.service.is_some() {
    match ScriptManager::start_service(&manifest.id, cx) {
      Ok(service) => registered._service = Some(service),
      Err(e) => tracing::error!("plugin service `{}`: {e:#}", manifest.id),
    }
  }
  for key in manifest.widgets.keys() {
    let name = format!("{}:{key}", manifest.id);
    let (id, key) = (manifest.id.clone(), key.clone());
    cx.bar_mut().register_data(WidgetData::from_fn(
      name.clone(),
      move |window, cx, _, options| match ScriptManager::load(
        &id,
        Entry::Widget(&key, options),
        window,
        cx,
      ) {
        Ok(script) => {
          let view = cx.new(|_| PluginView {
            script,
            fill: false,
          });
          // its panels open at it
          view.read(cx).script.set_opener(view.entity_id());
          Some(view.into())
        }
        Err(e) => {
          tracing::error!("plugin widget `{id}:{key}`: {e:#}");
          None
        }
      },
    ));
    registered.widgets.push(name);
  }
  for (key, panel) in &manifest.panels {
    let name = format!("{}:{key}", manifest.id);
    let (id, key) = (manifest.id.clone(), key.clone());
    let data = PanelData::from_fn(
      name.clone(),
      panel.width,
      panel.height,
      move |window, cx| match ScriptManager::load(&id, Entry::Panel(&key), window, cx) {
        Ok(script) => cx.new(|_| PluginView { script, fill: true }).into(),
        Err(e) => {
          tracing::error!("plugin panel `{id}:{key}`: {e:#}");
          cx.new(|_| PluginError(format!("{e:#}"))).into()
        }
      },
    );
    cx.global_mut::<PanelState>().register_data(data);
    registered.panels.push(name);
  }
  registered
}

/// Keeps a plugin's script, and so its watcher and subscriptions, alive as
/// long as the view is shown
struct PluginView {
  script: Script,
  /// A panel fills its window; a widget is as big as its content, so the bar
  /// knows where it is and its panels open there
  fill: bool,
}

impl Render for PluginView {
  fn render(&mut self, _: &mut Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
    div()
      .when(self.fill, |d| d.size_full())
      .child(self.script.view())
  }
}

struct PluginError(String);

impl Render for PluginError {
  fn render(&mut self, _: &mut Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
    div().p_2().child(self.0.clone())
  }
}

#[cfg(test)]
mod tests {
  use std::fs;

  use std::{cell::Cell, rc::Rc};

  use corona_components::components::tracked::Tracked;
  use corona_config::{Config, plugins::LOCAL_SOURCE};
  use gpui_kit::{self as gpui, Bounds, Pixels, TestAppContext, px};

  use super::*;
  use crate::{plugin::paths::Paths, test_support::setup};

  const VIEW: &str = r#"
import { View } from "corona/kit";
import { v_flex } from "corona/base";

export default class Main extends View {
  render(_cx) {
    return v_flex().child("plugin");
  }
}
"#;

  fn names(cx: &mut TestAppContext) -> Vec<String> {
    cx.update(|cx| {
      BarState::widget_names(cx)
        .into_iter()
        .filter(|n| n.contains(':'))
        .collect()
    })
  }

  #[test]
  fn tells_of_new_versions() {
    let v = |pairs: &[(&str, Option<&str>)]| -> Versions {
      pairs
        .iter()
        .map(|(id, version)| (id.to_string(), (id.to_uppercase(), version.map(Into::into))))
        .collect()
    };
    let none = HashSet::new();
    let mut versions = Versions::new();
    // the first scan and newly enabled plugins are no update
    assert!(updates(&mut versions, v(&[("a", Some("1")), ("b", None)]), &none).is_empty());
    assert_eq!(
      updates(
        &mut versions,
        v(&[("a", Some("2")), ("b", None), ("c", Some("1"))]),
        &none
      ),
      [("A".to_string(), "2".to_string())]
    );
    // waiting for approval, then approved
    let waiting = HashSet::from(["a".to_string()]);
    assert!(updates(&mut versions, v(&[("c", Some("1"))]), &waiting).is_empty());
    assert_eq!(
      updates(
        &mut versions,
        v(&[("a", Some("3")), ("c", Some("1"))]),
        &none
      ),
      [("A".to_string(), "3".to_string())]
    );
    // disabled, then enabled at another version: not told
    assert!(updates(&mut versions, v(&[("a", Some("3"))]), &none).is_empty());
    assert!(
      updates(
        &mut versions,
        v(&[("a", Some("3")), ("c", Some("2"))]),
        &none
      )
      .is_empty()
    );
  }

  #[gpui::test]
  fn registers_the_widgets_and_panels_of_running_plugins(cx: &mut TestAppContext) {
    setup(cx);
    let tmp = tempfile::tempdir().unwrap();
    let paths = Paths {
      state: tmp.path().join("state"),
      local: tmp.path().join("local"),
    };
    let dir = paths.local.join("clock");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
      dir.join("plugin.toml"),
      "id = \"com.clock\"\nname = \"Clock\"\napi = 2\n[widgets.bar]\nview = \"main.js\"\n[widgets.broken]\nview = \"missing.js\"\n[panels.main]\nview = \"main.js\"\nwidth = 200\n",
    )
    .unwrap();
    fs::write(dir.join("main.js"), VIEW).unwrap();

    cx.update(|cx| {
      corona_surface::init(cx).unwrap();
      let mut config = Config::default();
      config.plugins.enabled = vec!["com.clock".into()];
      config.plugins.approved.insert(
        "com.clock".into(),
        corona_config::plugins::Approval {
          source: "local".into(),
          ..Default::default()
        },
      );
      config.plugins.source.clear();
      cx.set_global(config);
      cx.set_global(ScriptManager::new(paths.clone()));
      PluginManager::init(paths.clone(), cx);
      init(cx);
    });
    assert_eq!(names(cx), ["com.clock:bar", "com.clock:broken"]);
    let found = cx.update(|cx| {
      cx.global::<PluginManager>().active()["com.clock"]
        .origin
        .clone()
    });
    assert_eq!(found.source, LOCAL_SOURCE);

    // a widget whose script loads is built, a broken one skipped
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
      assert!(PanelState::names(cx).contains(&"com.clock:main".to_string()));
      let ok = cx.bar().widget("com.clock:bar").cloned().unwrap();
      assert!(ok.init(window, cx, uuid::Uuid::nil(), None).is_some());
      let broken = cx.bar().widget("com.clock:broken").cloned().unwrap();
      assert!(broken.init(window, cx, uuid::Uuid::nil(), None).is_none());
    });

    // disabled: gone again
    cx.update(|_, cx| cx.global_mut::<Config>().plugins.enabled.clear());
    cx.run_until_parked();
    cx.update(|_, cx| {
      let names = BarState::widget_names(cx);
      assert!(names.iter().all(|n| !n.contains(':')), "{names:?}");
      assert!(!PanelState::names(cx).contains(&"com.clock:main".to_string()));
    });
  }

  /// Lays `view` out in a bar-sized row, like a bar does, and measures it
  struct Row(Option<gpui_kit::AnyView>, Rc<Cell<Bounds<Pixels>>>);

  impl Render for Row {
    fn render(&mut self, _: &mut Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
      div().flex().w(px(1000.)).h(px(40.)).children(
        self
          .0
          .clone()
          .map(|view| Tracked::new(view, self.1.clone())),
      )
    }
  }

  /// 40px wide, without text the test platform cannot measure
  const SIZED_VIEW: &str = r#"
import { View, div } from "corona/kit";

export default class Main extends View {
  render(_cx) {
    return div().w_10().h_4();
  }
}
"#;

  /// Echoes every call, with how often it started
  const SERVICE: &str = r#"
import { onCall, reply, getState, setState } from "corona/plugin";

export default function main(_cx) {
  const starts = (getState("starts") ?? 0) + 1;
  setState("starts", starts);
  onCall((call) => reply(call.id, [call.method, call.args, starts]));
}
"#;

  #[gpui::test]
  fn services_run_while_the_plugin_is_enabled(cx: &mut TestAppContext) {
    services_run(cx, "service.js", SERVICE);
  }

  #[gpui::test]
  fn typescript_services_run(cx: &mut TestAppContext) {
    let typed = SERVICE.replace("main(_cx)", "main(_cx: unknown): void");
    services_run(cx, "service.ts", &typed);
  }

  /// The service `file`, with `source`, through its plugin's whole lifecycle
  fn services_run(cx: &mut TestAppContext, file: &str, source: &str) {
    setup(cx);
    let tmp = tempfile::tempdir().unwrap();
    let paths = Paths {
      state: tmp.path().join("state"),
      local: tmp.path().join("local"),
    };
    let dir = paths.local.join("echo");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
      dir.join("plugin.toml"),
      format!("id = \"com.echo\"\nname = \"Echo\"\napi = 2\nservice = \"{file}\"\n"),
    )
    .unwrap();
    fs::write(dir.join(file), source).unwrap();
    cx.update(|cx| {
      corona_surface::init(cx).unwrap();
      let mut config = Config::default();
      config.plugins.enabled = vec!["com.echo".into()];
      config.plugins.approved.insert(
        "com.echo".into(),
        corona_config::plugins::Approval {
          source: "local".into(),
          ..Default::default()
        },
      );
      config.plugins.source.clear();
      cx.set_global(config);
      cx.set_global(ScriptManager::new(paths.clone()));
      PluginManager::init(paths, cx);
      init(cx);
    });
    cx.run_until_parked();
    let call = |cx: &mut TestAppContext| {
      let answer = cx.update(|cx| crate::call("com.echo", "ping".into(), 1.into(), cx));
      let answer = cx.executor().spawn(answer.unwrap());
      cx.run_until_parked();
      futures::executor::block_on(answer)
    };
    assert_eq!(call(cx).unwrap(), serde_json::json!(["ping", 1, 1]));
    // no window for it
    assert!(cx.windows().is_empty());

    // changed on disk: restarted
    fs::write(
      dir.join("plugin.toml"),
      format!(
        "id = \"com.echo\"\nname = \"Echo\"\napi = 2\nversion = \"2\"\nservice = \"{file}\"\n"
      ),
    )
    .unwrap();
    let revision = cx.update(|cx| cx.global::<PluginManager>().revision());
    cx.update(PluginManager::rescan);
    cx.run_until_parked();
    assert!(cx.update(|cx| cx.global::<PluginManager>().revision()) > revision);
    assert_eq!(call(cx).unwrap(), serde_json::json!(["ping", 1, 2]));

    // disabled: stopped
    cx.update(|cx| cx.global_mut::<Config>().plugins.enabled.clear());
    cx.run_until_parked();
    assert!(
      cx.update(|cx| crate::call("com.echo", "ping".into(), 1.into(), cx))
        .is_err()
    );
  }

  #[gpui::test]
  fn a_widget_is_as_big_as_its_content(cx: &mut TestAppContext) {
    setup(cx);
    let tmp = tempfile::tempdir().unwrap();
    let paths = Paths {
      state: tmp.path().join("state"),
      local: tmp.path().join("local"),
    };
    let dir = paths.local.join("clock");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
      dir.join("plugin.toml"),
      "id = \"com.clock\"\nname = \"Clock\"\napi = 2\n[widgets.bar]\nview = \"main.js\"\n",
    )
    .unwrap();
    fs::write(dir.join("main.js"), SIZED_VIEW).unwrap();
    cx.update(|cx| {
      corona_surface::init(cx).unwrap();
      let mut config = Config::default();
      config.plugins.enabled = vec!["com.clock".into()];
      config.plugins.approved.insert(
        "com.clock".into(),
        corona_config::plugins::Approval {
          source: "local".into(),
          ..Default::default()
        },
      );
      config.plugins.source.clear();
      cx.set_global(config);
      cx.set_global(ScriptManager::new(paths.clone()));
      PluginManager::init(paths, cx);
      init(cx);
    });
    let bounds = Rc::new(Cell::new(Bounds::default()));
    let measured = bounds.clone();
    let window = cx.add_window(move |_, _| Row(None, measured));
    let row = window.root(cx).unwrap();
    cx.update_window(window.into(), |_, window, cx| {
      let data = cx.bar().widget("com.clock:bar").cloned().unwrap();
      let view = data.init(window, cx, uuid::Uuid::nil(), None).unwrap();
      row.update(cx, |row, cx| {
        row.0 = Some(view);
        cx.notify();
      });
    })
    .unwrap();
    for _ in 0..2 {
      cx.update_window(window.into(), |_, window, cx| {
        use gpui_kit::test::TestWindowExt;
        window.render_frame(cx)
      })
      .unwrap();
      cx.run_until_parked();
    }
    let width = bounds.get().size.width;
    // the panel opens at the widget, so it must not stretch across the bar
    assert_eq!(width, px(40.));
  }
}
