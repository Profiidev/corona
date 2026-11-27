//! `corona/dashboard`: the service's side of the plugin's dashboard tiles and sliders. It
//! sets what they show, adds and removes its own, and hears of clicks and drags.

use std::{cell::Cell, time::Duration};

use corona_js::{
  Cx, Data, Json, Listener, ModuleBuilder, PluginRef, ScriptModule, Throws, Unsubscribe, Watches,
  host_module,
};
use corona_surface::dashboard::DashboardExt;
use gpui_kit::{App, Entity, Subscription};
use serde::{Deserialize, Serialize};
use specta::{Type, specta};

use crate::{
  ScriptManager,
  dashboard::{
    DashboardEvent, Kind, PluginDashboard, Range, SliderDef, SliderState, TileDef, TileState,
    slider_entry, tile_entry, unregister,
  },
  plugin::manifest::SliderRange,
};

pub struct Dashboard;

impl ScriptModule for Dashboard {
  const NAME: &'static str = "dashboard";
  const DOC: &'static str =
    "Set the state of the plugin's dashboard tiles and sliders, add more and follow clicks";

  fn module(plugin: &PluginRef, m: &mut ModuleBuilder, cx: &mut App) {
    let entity = ScriptManager::hub(&plugin.id, cx).dashboard.clone();
    build(entity, m);
  }
}

/// What one runtime of the plugin shares
struct Module {
  entity: Entity<PluginDashboard>,
  watches: Watches,
  started: Cell<bool>,
}

fn build(entity: Entity<PluginDashboard>, m: &mut ModuleBuilder) {
  m.provide(Module {
    entity,
    watches: m.watches().clone(),
    started: Cell::new(false),
  });
  host_module!(m, {
    sync: [
      set_tile, add_tile, remove_tile, set_slider, add_slider, remove_slider, on_toggle,
      on_set, on_mute, on_secondary
    ]
  });
}

impl Module {
  /// The first use starts a run of the service: what an earlier run added goes. When the
  /// runtime ends, so does the run.
  // shortcut: a view importing this module takes the run over; it is the service's only
  fn start(&self, cx: &mut App) {
    if self.started.replace(true) {
      return;
    }
    let (run, removed) = self.entity.update(cx, |d, cx| {
      cx.notify();
      d.start()
    });
    let id = self.entity.read(cx).id().to_string();
    unregister(&id, &removed, cx);
    let (entity, async_cx) = (self.entity.clone(), cx.to_async());
    self.watches.hold(Subscription::new(move || {
      // the runtime goes without an `App` at hand
      async_cx
        .spawn(async move |cx| {
          cx.update(|cx| {
            let removed = entity.update(cx, |d, cx| {
              cx.notify();
              d.stop(run)
            });
            if let Some(removed) = removed {
              let id = entity.read(cx).id().to_string();
              unregister(&id, &removed, cx);
            }
          })
        })
        .detach();
    }));
  }

  fn update<R>(
    &self,
    cx: &mut App,
    f: impl FnOnce(&mut PluginDashboard) -> anyhow::Result<R>,
  ) -> anyhow::Result<R> {
    self.start(cx);
    self.entity.update(cx, |d, cx| {
      let result = f(d)?;
      cx.notify();
      Ok(result)
    })
  }

  fn listen<T: for<'js> corona_js::rquickjs::IntoJs<'js> + 'static>(
    &self,
    cx: &mut App,
    cb: Listener<T>,
    pick: impl Fn(&DashboardEvent) -> Option<T> + 'static,
  ) -> Unsubscribe {
    self.start(cx);
    let listener = cb.clone();
    let sub = cx.subscribe(&self.entity, move |_, event: &DashboardEvent, cx| {
      if let Some(value) = pick(event) {
        listener.call(cx, value)
      }
    });
    cb.keep(sub)
  }
}

/// A tile the service adds: its title and icon, and optionally what it shows.
#[derive(Clone, Debug, Deserialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewTile {
  pub title: String,
  /// A bundled icon's kebab-case name.
  pub icon: String,
  #[specta(optional)]
  #[serde(default)]
  pub status: Option<String>,
  #[specta(optional)]
  #[serde(default)]
  pub active: Option<bool>,
  #[specta(optional)]
  #[serde(default)]
  pub enabled: Option<bool>,
}

/// A slider the service adds: its title, icon and range, and optionally its value.
#[derive(Clone, Debug, Deserialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewSlider {
  pub title: String,
  /// A bundled icon's kebab-case name.
  pub icon: String,
  /// The lowest value, with `max`; without a range the value goes from 0 to 1 and shows
  /// as a percent.
  #[specta(optional)]
  #[serde(default)]
  pub min: Option<f64>,
  #[specta(optional)]
  #[serde(default)]
  pub max: Option<f64>,
  /// What the value snaps to; a hundredth of the range when unset.
  #[specta(optional)]
  #[serde(default)]
  pub step: Option<f64>,
  /// Shown after the value, like `°C`; needs `min` and `max`.
  #[specta(optional)]
  #[serde(default)]
  pub unit: Option<String>,
  /// While dragging, `onSet` hears of the value at most this often, in milliseconds.
  #[specta(optional)]
  #[serde(default)]
  pub throttle_ms: Option<u32>,
  /// In the slider's range; hidden until set.
  #[specta(optional)]
  #[serde(default)]
  pub value: Option<f64>,
  /// Makes the icon a mute button, red while `true`.
  #[specta(optional)]
  #[serde(default)]
  pub muted: Option<bool>,
}

/// A slider moved to `value`, in its range.
#[derive(Clone, Debug, PartialEq, Serialize, Type)]
pub struct SetEvent {
  pub key: String,
  pub value: f64,
}

/// A right click on a tile, or the chevron of a slider.
#[derive(Clone, Debug, PartialEq, Serialize, Type)]
pub struct SecondaryEvent {
  pub kind: Kind,
  pub key: String,
}

fn icon(name: &str) -> anyhow::Result<gpui_kit::assets::IconName> {
  corona_launcher::icon_by_name(name).ok_or_else(|| anyhow::anyhow!("unknown icon `{name}`"))
}

/// Sets what tile `key` shows, a manifest one or one `addTile` added; the fields left
/// out stay. A manifest tile takes clicks once this was called.
#[specta]
fn set_tile(mut cx: Cx, module: Data<Module>, key: String, state: Json<TileState>) -> Throws<()> {
  module.update(&mut cx, |d| d.set_tile(&key, state.0)).into()
}

/// Adds tile `<id>:<key>` to the dashboard's choices, or replaces one added before. The
/// user puts it on the dashboard in the settings app. It goes when the service stops.
#[specta]
fn add_tile(mut cx: Cx, module: Data<Module>, key: String, tile: Json<NewTile>) -> Throws<()> {
  let add = |cx: &mut App| -> anyhow::Result<()> {
    let tile = tile.0;
    let def = TileDef {
      title: tile.title,
      icon: icon(&tile.icon)?,
    };
    let state = TileState {
      title: None,
      icon: None,
      status: tile.status,
      active: tile.active,
      enabled: tile.enabled,
    };
    module.update(cx, |d| d.add_tile(&key, def, state))?;
    let entry = tile_entry(&module.entity, &key, cx);
    cx.dashboard_mut().quick_action_entry(entry);
    Ok(())
  };
  add(&mut cx).into()
}

/// Removes a tile `addTile` added; nothing for an unknown key.
#[specta]
fn remove_tile(mut cx: Cx, module: Data<Module>, key: String) -> Throws<()> {
  let remove = |cx: &mut App| -> anyhow::Result<()> {
    if module.update(cx, |d| d.remove_tile(&key))? {
      let id = module.entity.read(cx).id().to_string();
      cx.dashboard_mut()
        .remove_quick_action(&crate::dashboard::item_id(&id, &key));
    }
    Ok(())
  };
  remove(&mut cx).into()
}

/// Sets what slider `key` shows, a manifest one or one `addSlider` added; the fields
/// left out stay.
#[specta]
fn set_slider(
  mut cx: Cx,
  module: Data<Module>,
  key: String,
  state: Json<SliderState>,
) -> Throws<()> {
  module
    .update(&mut cx, |d| d.set_slider(&key, state.0))
    .into()
}

/// Adds slider `<id>:<key>` to the dashboard's choices, or replaces one added before. The
/// user puts it on the dashboard in the settings app. It goes when the service stops.
#[specta]
fn add_slider(
  mut cx: Cx,
  module: Data<Module>,
  key: String,
  slider: Json<NewSlider>,
) -> Throws<()> {
  let add = |cx: &mut App| -> anyhow::Result<()> {
    let slider = slider.0;
    let range = Range::new(&SliderRange {
      min: slider.min,
      max: slider.max,
      step: slider.step,
      unit: slider.unit,
    })?;
    let def = SliderDef {
      title: slider.title,
      icon: icon(&slider.icon)?,
      range,
      throttle: slider
        .throttle_ms
        .map(|ms| Duration::from_millis(ms.into())),
    };
    let state = SliderState {
      title: None,
      icon: None,
      value: slider.value.map(Some),
      muted: slider.muted.map(Some),
    };
    module.update(cx, |d| d.add_slider(&key, def, state))?;
    if let Some(entry) = slider_entry(&module.entity, &key, cx) {
      cx.dashboard_mut().slider_entry(entry);
    }
    Ok(())
  };
  add(&mut cx).into()
}

/// Removes a slider `addSlider` added; nothing for an unknown key.
#[specta]
fn remove_slider(mut cx: Cx, module: Data<Module>, key: String) -> Throws<()> {
  let remove = |cx: &mut App| -> anyhow::Result<()> {
    if module.update(cx, |d| d.remove_slider(&key))? {
      let id = module.entity.read(cx).id().to_string();
      cx.dashboard_mut()
        .remove_slider(&crate::dashboard::item_id(&id, &key));
    }
    Ok(())
  };
  remove(&mut cx).into()
}

/// Calls `cb` with the key of each tile clicked.
#[specta]
fn on_toggle(mut cx: Cx, module: Data<Module>, cb: Listener<String>) -> Unsubscribe {
  module.listen(&mut cx, cb, |event| match event {
    DashboardEvent::Toggle(key) => Some(key.clone()),
    _ => None,
  })
}

/// Calls `cb` as a slider moves, with its value in its range.
#[specta]
fn on_set(mut cx: Cx, module: Data<Module>, cb: Listener<Json<SetEvent>>) -> Unsubscribe {
  module.listen(&mut cx, cb, |event| match event {
    DashboardEvent::Set(key, value) => Some(Json(SetEvent {
      key: key.clone(),
      value: *value,
    })),
    _ => None,
  })
}

/// Calls `cb` with the key of each slider whose mute button was clicked.
#[specta]
fn on_mute(mut cx: Cx, module: Data<Module>, cb: Listener<String>) -> Unsubscribe {
  module.listen(&mut cx, cb, |event| match event {
    DashboardEvent::Mute(key) => Some(key.clone()),
    _ => None,
  })
}

/// Calls `cb` on a right click on a tile and on a slider's chevron, e.g. to open a panel.
#[specta]
fn on_secondary(
  mut cx: Cx,
  module: Data<Module>,
  cb: Listener<Json<SecondaryEvent>>,
) -> Unsubscribe {
  module.listen(&mut cx, cb, |event| match event {
    DashboardEvent::Secondary(kind, key) => Some(Json(SecondaryEvent {
      kind: *kind,
      key: key.clone(),
    })),
    _ => None,
  })
}

#[cfg(test)]
mod tests {
  use gpui_kit::{self as gpui, AppContext as _, TestAppContext, VisualContext as _};
  use serde_json::{Value, json};

  use super::*;
  use corona_surface::dashboard::Secondary;

  use crate::{dashboard::tests::manifest, module::harness};

  const ITEMS: &str = r#"
[quick_actions.lamp]
name = "Lamp"
icon = "lightbulb"
[sliders.heat]
name = "Heat"
icon = "thermometer"
min = 10
max = 30
"#;

  /// A plugin dashboard with the manifest's items on the dashboard
  fn dashboard(cx: &mut TestAppContext) -> Entity<PluginDashboard> {
    cx.update(|cx| {
      let entity = crate::dashboard::new_entity("p", cx);
      crate::dashboard::register(&entity, &manifest(ITEMS), cx);
      entity
    })
  }

  /// A runtime of the module, which ends with its window
  struct Run {
    view: harness::View,
    window: gpui_kit::AnyWindowHandle,
  }

  impl Run {
    fn reports(&self, cx: &mut TestAppContext) -> Vec<Value> {
      self.view.reports(cx)
    }

    /// Ends the runtime, as a service stopping does
    fn stop(self, cx: &mut TestAppContext) {
      cx.update_window(self.window, |_, window, _| window.remove_window())
        .unwrap();
      drop(self.view);
      // the view's entity is released on the next flush
      cx.update(|_| {});
      cx.run_until_parked();
    }
  }

  /// Runs `body` once in a view of the module over `entity`
  fn run(cx: &mut TestAppContext, entity: &Entity<PluginDashboard>, body: &str) -> Run {
    let body = format!("if (!globalThis.started) {{ globalThis.started = true; {body} }}");
    let entity = entity.clone();
    let (view, vcx) = harness::view(cx, "corona/dashboard", &body, move |_, m, _| {
      build(entity, m)
    });
    vcx.run_until_parked();
    let window = vcx.window_handle();
    Run { view, window }
  }

  fn ids(cx: &mut TestAppContext) -> (Vec<String>, Vec<String>) {
    cx.update(|cx| {
      let registry = cx.dashboard();
      (
        (registry.quick_actions().iter())
          .map(|a| a.id.to_string())
          .collect(),
        (registry.sliders().iter())
          .map(|s| s.id.to_string())
          .collect(),
      )
    })
  }

  fn tile(cx: &mut TestAppContext, id: &str) -> corona_surface::dashboard::QuickActionEntry {
    cx.update(|cx| {
      (cx.dashboard().quick_actions().iter())
        .find(|a| a.id == id)
        .cloned()
        .unwrap()
    })
  }

  fn slider(cx: &mut TestAppContext, id: &str) -> corona_surface::dashboard::SliderEntry {
    cx.update(|cx| {
      (cx.dashboard().sliders().iter())
        .find(|s| s.id == id)
        .cloned()
        .unwrap()
    })
  }

  /// The declarations plugins get
  fn declarations() -> String {
    let watches = Watches::default();
    let mut m = ModuleBuilder::new("corona/dashboard", &watches);
    let entity =
      gpui_kit::TestAppContext::single().update(|cx| crate::dashboard::new_entity("p", cx));
    build(entity, &mut m);
    corona_js::d_ts([&m])
  }

  #[test]
  fn the_declarations_are_generated_from_the_types() {
    let dts = declarations();
    for expected in [
      "declare module \"corona/dashboard\"",
      "export function setTile(key: string, state: TileState): void;",
      "export function addTile(key: string, tile: NewTile): void;",
      "export function removeTile(key: string): void;",
      "export function setSlider(key: string, state: SliderState): void;",
      "export function addSlider(key: string, slider: NewSlider): void;",
      "export function removeSlider(key: string): void;",
      "export function onToggle(cb: (value: string) => void): () => void;",
      "export function onSet(cb: (value: SetEvent) => void): () => void;",
      "export function onMute(cb: (value: string) => void): () => void;",
      "export function onSecondary(cb: (value: SecondaryEvent) => void): () => void;",
      "export type TileState = {",
      "value?: number | null",
      "throttleMs?: number | null",
      "export type Kind = \"tile\" | \"slider\";",
      // docs become JSDoc
      "Makes the icon a mute button",
    ] {
      assert!(dts.contains(expected), "missing {expected:?} in\n{dts}");
    }
  }

  #[gpui::test]
  fn the_service_sets_manifest_items_and_hears_clicks(cx: &mut TestAppContext) {
    let entity = dashboard(cx);
    let view = run(
      cx,
      &entity,
      r#"
      m.setTile("lamp", { status: "On", active: true });
      m.setSlider("heat", { value: 15, muted: false });
      m.onToggle((key) => report({ toggle: key }));
      m.onSet((e) => report({ set: e }));
      m.onMute((key) => report({ mute: key }));
      m.onSecondary((e) => report({ secondary: e }));
      "#,
    );
    let lamp = tile(cx, "p:lamp");
    let heat = slider(cx, "p:heat");
    cx.update(|cx| {
      let look = (lamp.tile)(cx);
      assert_eq!((look.status.as_ref(), look.active), ("On", true));
      (look.toggle.expect("the service set it"))(cx);
      assert_eq!((heat.value)(cx), Some(0.25));
      assert_eq!((heat.muted)(cx), Some(false));
      (heat.set)(0.5, cx);
      (heat.toggle_mute)(cx);
      for secondary in [lamp.secondary.clone(), heat.secondary.clone()] {
        let Some(Secondary::Run(run)) = secondary else {
          panic!("plugins run a fn")
        };
        run(cx);
      }
    });
    cx.run_until_parked();
    assert_eq!(
      view.reports(cx),
      [
        json!({ "toggle": "lamp" }),
        json!({ "set": { "key": "heat", "value": 20 } }),
        json!({ "mute": "heat" }),
        json!({ "secondary": { "kind": "tile", "key": "lamp" } }),
        json!({ "secondary": { "kind": "slider", "key": "heat" } }),
      ]
    );

    // the runtime ends: the manifest's items wait for the next run
    view.stop(cx);
    cx.update(|cx| {
      let look = (lamp.tile)(cx);
      assert!(look.toggle.is_none() && !look.active);
      assert_eq!((heat.value)(cx), None);
    });
    assert_eq!(ids(cx), (vec!["p:lamp".into()], vec!["p:heat".into()]));
  }

  #[gpui::test]
  fn the_service_adds_items_that_go_with_it(cx: &mut TestAppContext) {
    let entity = dashboard(cx);
    let view = run(
      cx,
      &entity,
      r#"
      m.addTile("fan", { title: "Fan", icon: "fan", status: "Off" });
      m.addTile("gone", { title: "Gone", icon: "bell" });
      m.removeTile("gone");
      m.removeTile("never");
      m.addSlider("vol", { title: "Volume", icon: "volume-2", min: 0, max: 10, step: 1, unit: "dB", value: 4, muted: true, throttleMs: 50 });
      m.addSlider("tmp", { title: "Tmp", icon: "sun" });
      m.removeSlider("tmp");
      m.removeSlider("never");
      m.onToggle((key) => report(key));
      "#,
    );
    assert_eq!(
      ids(cx),
      (
        vec!["p:lamp".into(), "p:fan".into()],
        vec!["p:heat".into(), "p:vol".into()]
      )
    );
    let fan = tile(cx, "p:fan");
    let vol = slider(cx, "p:vol");
    cx.update(|cx| {
      assert_eq!((fan.title)(cx), "Fan");
      let look = (fan.tile)(cx);
      assert_eq!(look.status, "Off");
      (look.toggle.expect("added tiles take clicks"))(cx);
      assert_eq!((vol.value)(cx), Some(0.4));
      assert_eq!((vol.muted)(cx), Some(true));
      assert_eq!((vol.label)(0.4, cx), "4 dB");
      assert_eq!(vol.step, 0.1);
      assert_eq!(vol.throttle, Some(Duration::from_millis(50)));
    });
    cx.run_until_parked();
    assert_eq!(view.reports(cx), [json!("fan")]);

    view.stop(cx);
    assert_eq!(ids(cx), (vec!["p:lamp".into()], vec!["p:heat".into()]));
  }

  #[gpui::test]
  fn mistakes_throw(cx: &mut TestAppContext) {
    let entity = dashboard(cx);
    let view = run(
      cx,
      &entity,
      r#"
      for (const f of [
        () => m.setTile("nope", {}),
        () => m.setTile("lamp", { icon: "no-such-icon" }),
        () => m.setTile("lamp", { bogus: 1 }),
        () => m.setSlider("nope", { value: 1 }),
        () => m.addTile("lamp", { title: "L", icon: "bell" }),
        () => m.addTile("x", { title: "X", icon: "no-such-icon" }),
        () => m.addTile("a:b", { title: "X", icon: "bell" }),
        () => m.removeTile("lamp"),
        () => m.addSlider("heat", { title: "H", icon: "sun" }),
        () => m.addSlider("x", { title: "X", icon: "sun", min: 1 }),
        () => m.addSlider("x", { title: "X", icon: "sun", unit: "%" }),
        () => m.removeSlider("heat"),
      ]) {
        report(attempt(f));
      }
      "#,
    );
    let messages: Vec<String> = (view.reports(cx).into_iter())
      .map(|r| match r {
        Value::Object(o) => o["message"].as_str().unwrap_or_default().to_string(),
        other => panic!("did not throw: {other}"),
      })
      .collect();
    let expected = [
      "no tile `nope`",
      "unknown icon `no-such-icon`",
      "unknown field `bogus`",
      "no slider `nope`",
      "`lamp` is in plugin.toml",
      "unknown icon `no-such-icon`",
      "invalid key `a:b`",
      "tile `lamp` is in plugin.toml, it cannot be removed",
      "`heat` is in plugin.toml",
      "`min` and `max` go together",
      "`unit` needs `min` and `max`",
      "slider `heat` is in plugin.toml, it cannot be removed",
    ];
    assert_eq!(messages.len(), expected.len(), "{messages:?}");
    for (message, expected) in messages.iter().zip(expected) {
      assert!(message.contains(expected), "{message:?} lacks {expected:?}");
    }
    // nothing was added
    assert_eq!(ids(cx), (vec!["p:lamp".into()], vec!["p:heat".into()]));
  }

  #[gpui::test]
  fn a_new_run_drops_the_last_runs_items(cx: &mut TestAppContext) {
    let entity = dashboard(cx);
    let first = run(
      cx,
      &entity,
      r#"m.addTile("old", { title: "Old", icon: "bell" });"#,
    );
    assert!(ids(cx).0.contains(&"p:old".to_string()));
    // the next run starts before the old runtime is gone, as a restart does
    let second = run(
      cx,
      &entity,
      r#"m.addTile("new", { title: "New", icon: "bell" });"#,
    );
    assert_eq!(ids(cx).0, ["p:lamp", "p:new"]);
    first.stop(cx);
    assert_eq!(
      ids(cx).0,
      ["p:lamp", "p:new"],
      "a stopped run must not stop the next"
    );
    second.stop(cx);
    assert_eq!(ids(cx).0, ["p:lamp"]);
  }
}
