use anyhow::Result;
use corona_config::APP_NAME;
use corona_script::{Entry, PluginManager, Script, ScriptManager};
use corona_surface::{
  bar::{BarExt, BarState, WidgetData},
  panel::{PanelData, PanelState},
};
use gpui_kit::{
  AnyWindowHandle, App, AppContext, Bounds, IntoElement, ParentElement, Point, Render, Styled,
  Window, WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowKind, WindowOptions,
  div,
  layer_shell::{KeyboardInteractivity, Layer, LayerShellOptions},
  prelude::FluentBuilder,
  px, size,
};

/// The widget and panel names registered for plugins, and their services
#[derive(Default)]
struct Registered {
  widgets: Vec<String>,
  panels: Vec<String>,
  services: Vec<Script>,
  /// Where the services run, never shown
  window: Option<AnyWindowHandle>,
}

pub fn init(cx: &mut App) {
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
      let id = manifest.id.clone();
      let loaded = service_window(registered, cx).and_then(|window| {
        window.update(cx, |_, window, cx| {
          ScriptManager::load(&id, Entry::Service, window, cx)
        })?
      });
      match loaded {
        Ok(script) => registered.services.push(script),
        Err(e) => tracing::error!("plugin service `{id}`: {e:#}"),
      }
    }
    for key in manifest.widgets.keys() {
      let name = format!("{}:{key}", manifest.id);
      let (id, key) = (manifest.id.clone(), key.clone());
      cx.bar_mut().register_data(WidgetData::from_fn(
        name.clone(),
        move |window, cx, _, _| match ScriptManager::load(&id, Entry::Widget(&key), window, cx) {
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

/// A 1×1 window on the background layer that takes no input, for services:
/// their tasks resume in the window they were loaded in
fn service_window(registered: &mut Registered, cx: &mut App) -> Result<AnyWindowHandle> {
  if let Some(window) = registered.window
    && window.update(cx, |_, _, _| ()).is_ok()
  {
    return Ok(window);
  }
  let window = cx.open_window(
    WindowOptions {
      kind: WindowKind::LayerShell(LayerShellOptions {
        layer: Layer::Background,
        namespace: "corona-plugin-services".to_string(),
        keyboard_interactivity: KeyboardInteractivity::None,
        ..Default::default()
      }),
      window_background: WindowBackgroundAppearance::Transparent,
      window_decorations: Some(WindowDecorations::Client),
      app_id: Some(APP_NAME.to_string()),
      titlebar: None,
      window_bounds: Some(WindowBounds::Windowed(Bounds {
        origin: Point::default(),
        size: size(px(1.), px(1.)),
      })),
      ..Default::default()
    },
    |window, cx| {
      window.set_input_region(Some(&[]));
      cx.new(|_| Empty)
    },
  )?;
  registered.window = Some(window.into());
  Ok(window.into())
}

struct Empty;

impl Render for Empty {
  fn render(&mut self, _: &mut Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
    div()
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
      config.plugins.source.clear();
      cx.set_global(config);
      let runtime = gpui_shell::ShellRuntime::new_isolated_with_components(
        gpui_component_shell::components().unwrap(),
      )
      .unwrap();
      cx.set_global(ScriptManager::new(runtime, paths.clone()));
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

  /// Echoes every call
  const SERVICE: &str = r#"
import { View } from "gpui-kit";
import { v_flex } from "gpui-base";
import { nextCall, reply } from "corona/plugin";

export default class Service extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      while (true) {
        const call = await nextCall();
        if (call.message) break;
        reply(call.id, [call.method, call.args]);
      }
    });
  }
  render() { return v_flex(); }
}
"#;

  #[gpui::test]
  fn services_run_while_the_plugin_is_enabled(cx: &mut TestAppContext) {
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
      "id = \"com.echo\"\nname = \"Echo\"\n[service]\nview = \"service.js\"\n",
    )
    .unwrap();
    fs::write(dir.join("service.js"), SERVICE).unwrap();
    cx.update(|cx| {
      corona_surface::init(cx).unwrap();
      let mut config = Config::default();
      config.plugins.enabled = vec!["com.echo".into()];
      config.plugins.source.clear();
      cx.set_global(config);
      let runtime = gpui_shell::ShellRuntime::new_isolated_with_components(
        gpui_component_shell::components().unwrap(),
      )
      .unwrap();
      cx.set_global(ScriptManager::new(runtime, paths.clone()));
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
    assert_eq!(call(cx).unwrap(), serde_json::json!(["ping", 1]));

    // changed on disk: restarted
    fs::write(
      dir.join("plugin.toml"),
      "id = \"com.echo\"\nname = \"Echo\"\nversion = \"2\"\n[service]\nview = \"service.js\"\n",
    )
    .unwrap();
    let revision = cx.update(|cx| cx.global::<PluginManager>().revision());
    cx.update(PluginManager::rescan);
    cx.run_until_parked();
    assert!(cx.update(|cx| cx.global::<PluginManager>().revision()) > revision);
    assert_eq!(call(cx).unwrap(), serde_json::json!(["ping", 1]));

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
      config.plugins.source.clear();
      cx.set_global(config);
      let runtime = gpui_shell::ShellRuntime::new_isolated_with_components(
        gpui_component_shell::components().unwrap(),
      )
      .unwrap();
      cx.set_global(ScriptManager::new(runtime, paths.clone()));
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
