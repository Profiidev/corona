use std::{
  cell::RefCell,
  collections::{HashMap, HashSet},
  rc::Rc,
};

use corona_notifications::{Action, Notifications, Notify, Urgency};
use corona_script::{Entry, PluginManager, Script, ScriptManager, Service};
use corona_surface::{
  bar::{BarExt, BarState, WidgetData},
  panel::{PanelData, PanelState},
};
use gpui_kit::{
  App, AppContext, IntoElement, ParentElement, Render, Styled, Window, div, prelude::FluentBuilder,
};
use rust_i18n::t;

/// The widget and panel names registered for plugins, and their services
#[derive(Default)]
struct Registered {
  widgets: Vec<String>,
  panels: Vec<String>,
  services: Vec<Service>,
}

pub fn init(cx: &mut App) {
  watch_approvals(cx);
  let mut registered = Registered::default();
  sync(&mut registered, cx);
  let mut revision = cx.global::<PluginManager>().revision();
  cx.observe_global::<PluginManager>(move |cx| {
    let now = cx.global::<PluginManager>().revision();
    if now != revision {
      revision = now;
      sync(&mut registered, cx);
      BarState::reopen(cx);
    }
  })
  .detach();
}

/// Tells of plugin updates, and asks about each plugin as it starts waiting
/// for approval: in the settings window when open, else with a notification
fn watch_approvals(cx: &mut App) {
  // notification id to plugin id
  let sent = Rc::new(RefCell::new(HashMap::new()));
  let mut waiting = HashSet::new();
  // the running plugins' versions, to tell of updates
  let mut versions = Versions::new();
  let mut check = {
    let sent = sent.clone();
    move |cx: &mut App| {
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
        notify_updated(&name, &version, cx);
      }
      for id in now.difference(&waiting) {
        if !crate::settings::review_plugin(id, cx) {
          notify(id, sent.clone(), cx);
        }
      }
      waiting = now;
    }
  };
  check(cx);
  cx.observe_global::<PluginManager>(check).detach();

  let Some(actions) = cx.try_global::<Notifications>().map(|n| n.action_invoked()) else {
    return;
  };
  cx.spawn(async move |cx| {
    let mut actions = match actions.await {
      Ok(actions) => std::pin::pin!(actions),
      Err(e) => return tracing::error!("plugin approval notifications: {e:#}"),
    };
    use futures::StreamExt;
    while let Some((notification, _)) = actions.next().await {
      let Some(id) = sent.borrow_mut().remove(&notification) else {
        continue;
      };
      cx.update(|cx| {
        if let Err(e) = crate::settings::open(Some("plugins"), cx) {
          tracing::error!("opening the settings: {e:#}");
        }
        crate::settings::review_plugin(&id, cx);
      });
    }
  })
  .detach();
}

/// Clicking the notification, no button
const REVIEW: &str = "default";

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

/// Tells that plugin `name` runs `version` now
fn notify_updated(name: &str, version: &str, cx: &mut App) {
  let Some(notifications) = cx.try_global::<Notifications>() else {
    return;
  };
  let send = notifications.send(
    t!("app.settings.plugins.updated.title", name = name).into(),
    t!("app.settings.plugins.updated.body", version = version).into(),
  );
  cx.spawn(async move |_| {
    if let Err(e) = send.await {
      tracing::error!("telling of a plugin update: {e:#}");
    }
  })
  .detach();
}

fn notify(id: &str, sent: Rc<RefCell<HashMap<u32, String>>>, cx: &mut App) {
  let Some(notifications) = cx.try_global::<Notifications>() else {
    return;
  };
  let name = cx.global::<PluginManager>().unapproved()[id]
    .manifest
    .name
    .clone();
  let send = notifications.send_notify(Notify {
    app_name: corona_config::APP_NAME.into(),
    app_icon: String::new(),
    summary: t!("app.settings.plugins.approve.changed_title", name = name).into(),
    body: t!("app.settings.plugins.approve.changed_body").into(),
    actions: vec![Action {
      key: REVIEW.into(),
      label: t!("app.settings.plugins.approve.review").into(),
    }],
    urgency: Urgency::Normal,
    hints: HashMap::new(),
    // the default popup time, then it waits in the notification center
    timeout: -1,
  });
  let id = id.to_string();
  cx.spawn(async move |_| match send.await {
    Ok(notification) => drop(sent.borrow_mut().insert(notification, id)),
    Err(e) => tracing::error!("asking to approve plugin `{id}`: {e:#}"),
  })
  .detach();
}

fn sync(registered: &mut Registered, cx: &mut App) {
  // stops them, failing what waits for them
  registered.services.clear();
  for name in registered.widgets.drain(..) {
    cx.bar_mut().unregister(&name);
  }
  for name in registered.panels.drain(..) {
    PanelState::unregister(&name, cx);
  }

  let plugins: Vec<_> = cx
    .global::<PluginManager>()
    .active()
    .values()
    .map(|found| found.manifest.clone())
    .collect();
  for manifest in plugins {
    if manifest.service.is_some() {
      match ScriptManager::start_service(&manifest.id, cx) {
        Ok(service) => registered.services.push(service),
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
  }
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

  use corona_config::{Config, plugins::LOCAL_SOURCE};
  use corona_script::plugin::paths::Paths;
  use gpui_kit::{self as gpui, TestAppContext};

  use std::{cell::Cell, rc::Rc};

  use corona_components::components::tracked::Tracked;
  use gpui_kit::{Bounds, Pixels, px};

  use super::*;
  use crate::test_support::{FakeCompositor, setup};

  const VIEW: &str = r#"
import { View } from "gpui-kit";
import { v_flex } from "gpui-base";

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
    setup(FakeCompositor::default(), cx);
    let tmp = tempfile::tempdir().unwrap();
    let paths = Paths {
      state: tmp.path().join("state"),
      local: tmp.path().join("local"),
    };
    let dir = paths.local.join("clock");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
      dir.join("plugin.toml"),
      "id = \"com.clock\"\nname = \"Clock\"\n[widgets.bar]\nview = \"main.js\"\n[widgets.broken]\nview = \"missing.js\"\n[panels.main]\nview = \"main.js\"\nwidth = 200\n",
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
import { View, div } from "gpui-kit";

export default class Main extends View {
  render(_cx) {
    return div().w_10().h_4();
  }
}
"#;

  /// Echoes every call, with how often it started
  const SERVICE: &str = r#"
import { nextCall, reply, getState, setState } from "corona/plugin";

export default async function main(_cx) {
  const starts = (getState("starts") ?? 0) + 1;
  setState("starts", starts);
  while (true) {
    const call = await nextCall();
    if (call.message) break;
    reply(call.id, [call.method, call.args, starts]);
  }
}
"#;

  #[gpui::test]
  fn services_run_while_the_plugin_is_enabled(cx: &mut TestAppContext) {
    services_run(cx, "service.js", SERVICE);
  }

  #[gpui::test]
  fn typescript_services_run(cx: &mut TestAppContext) {
    let typed = SERVICE.replace("main(_cx)", "main(_cx: unknown): Promise<void>");
    services_run(cx, "service.ts", &typed);
  }

  /// The service `file`, with `source`, through its plugin's whole lifecycle
  fn services_run(cx: &mut TestAppContext, file: &str, source: &str) {
    setup(FakeCompositor::default(), cx);
    let tmp = tempfile::tempdir().unwrap();
    let paths = Paths {
      state: tmp.path().join("state"),
      local: tmp.path().join("local"),
    };
    let dir = paths.local.join("echo");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
      dir.join("plugin.toml"),
      format!("id = \"com.echo\"\nname = \"Echo\"\nservice = \"{file}\"\n"),
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
      let answer = cx.update(|cx| corona_script::call("com.echo", "ping".into(), 1.into(), cx));
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
      format!("id = \"com.echo\"\nname = \"Echo\"\nversion = \"2\"\nservice = \"{file}\"\n"),
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
      cx.update(|cx| corona_script::call("com.echo", "ping".into(), 1.into(), cx))
        .is_err()
    );
  }

  #[gpui::test]
  fn a_widget_is_as_big_as_its_content(cx: &mut TestAppContext) {
    setup(FakeCompositor::default(), cx);
    let tmp = tempfile::tempdir().unwrap();
    let paths = Paths {
      state: tmp.path().join("state"),
      local: tmp.path().join("local"),
    };
    let dir = paths.local.join("clock");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
      dir.join("plugin.toml"),
      "id = \"com.clock\"\nname = \"Clock\"\n[widgets.bar]\nview = \"main.js\"\n",
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
