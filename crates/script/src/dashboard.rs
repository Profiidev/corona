//! Plugin tiles and sliders on the control center's dashboard: the `[quick_actions]` and
//! `[sliders]` of the manifest and those the service adds, with the state the service sets
//! through `corona/dashboard`. Each one is a [`corona_surface::dashboard`] entry named
//! `<id>:<key>` that reads a plugin's [`PluginDashboard`].

use std::{collections::BTreeMap, rc::Rc, time::Duration};

use anyhow::{Result, anyhow, bail, ensure};
use corona_launcher::icon_by_name;
use corona_surface::dashboard::{
  Changed, DashboardExt, QuickActionEntry, Secondary, SliderEntry, Tile, percent,
};
use gpui_kit::{App, AppContext, Context, Entity, EventEmitter, SharedString, assets::IconName};
use serde::{Deserialize, Deserializer, Serialize};
use specta::Type;

use crate::plugin::manifest::{PluginManifest, SliderRange};

/// A tile's title and icon, from the manifest or `addTile`
#[derive(Clone, Debug, PartialEq)]
pub struct TileDef {
  pub title: String,
  pub icon: IconName,
}

/// A slider's title, icon and range, from the manifest or `addSlider`
#[derive(Clone, Debug, PartialEq)]
pub struct SliderDef {
  pub title: String,
  pub icon: IconName,
  pub range: Range,
  pub throttle: Option<Duration>,
}

/// A slider's checked range. The dashboard's slider goes from 0 to 1; this maps it to the
/// plugin's values.
#[derive(Clone, Debug, PartialEq)]
pub struct Range {
  min: f64,
  max: f64,
  step: f64,
  unit: Option<String>,
  /// Shows the value with its unit, not a percent
  scaled: bool,
}

impl Range {
  pub fn new(range: &SliderRange) -> Result<Self> {
    range.check()?;
    let (min, max, scaled) = match (range.min, range.max) {
      (Some(min), Some(max)) => (min, max, true),
      _ => (0., 1., false),
    };
    Ok(Self {
      min,
      max,
      step: range.step.unwrap_or((max - min) / 100.),
      unit: range.unit.clone(),
      scaled,
    })
  }

  /// `value` on the 0 to 1 scale
  pub fn normalize(&self, value: f64) -> f32 {
    ((value - self.min) / (self.max - self.min)).clamp(0., 1.) as f32
  }

  /// The plugin's value at `level` from 0 to 1, snapped to the step from `min`
  pub fn value(&self, level: f32) -> f64 {
    let raw = self.min + f64::from(level.clamp(0., 1.)) * (self.max - self.min);
    let snapped = self.min + ((raw - self.min) / self.step).round() * self.step;
    snapped.clamp(self.min, self.max)
  }

  /// The step on the 0 to 1 scale
  pub fn level_step(&self) -> f32 {
    ((self.step / (self.max - self.min)) as f32).clamp(f32::EPSILON, 1.)
  }

  /// What shows right of the slider at `level`: the value and its unit, or a percent
  pub fn label(&self, level: f32) -> SharedString {
    if !self.scaled {
      return percent(level);
    }
    let value = format!("{:.6}", self.value(level));
    let value = value.trim_end_matches('0').trim_end_matches('.');
    let value = if value == "-0" { "0" } else { value };
    match self.unit.as_deref() {
      Some(unit) if !unit.is_empty() => format!("{value} {unit}").into(),
      _ => value.to_string().into(),
    }
  }
}

/// `Some` for a field that is present, so a `null` in it is told apart from leaving it out
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
  deserializer: D,
) -> Result<Option<Option<T>>, D::Error> {
  Option::deserialize(deserializer).map(Some)
}

/// What a tile shows; `setTile` changes the fields it names and keeps the others.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TileState {
  /// Replaces the manifest's `name` as the title, e.g. translated.
  #[specta(optional)]
  #[serde(default)]
  pub title: Option<String>,
  /// A bundled icon's kebab-case name.
  #[specta(optional)]
  #[serde(default)]
  pub icon: Option<String>,
  /// The line under the title, like `On` or a room's temperature.
  #[specta(optional)]
  #[serde(default)]
  pub status: Option<String>,
  /// Highlights the tile.
  #[specta(optional)]
  #[serde(default)]
  pub active: Option<bool>,
  /// `false` greys the tile out and ignores clicks.
  #[specta(optional)]
  #[serde(default)]
  pub enabled: Option<bool>,
}

impl TileState {
  fn merge(&mut self, new: TileState) {
    let TileState {
      title,
      icon,
      status,
      active,
      enabled,
    } = new;
    self.title = title.or(self.title.take());
    self.icon = icon.or(self.icon.take());
    self.status = status.or(self.status.take());
    self.active = active.or(self.active);
    self.enabled = enabled.or(self.enabled);
  }
}

/// What a slider shows; `setSlider` changes the fields it names and keeps the others.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SliderState {
  /// Replaces the manifest's `name` as the title, e.g. translated.
  #[specta(optional)]
  #[serde(default)]
  pub title: Option<String>,
  /// A bundled icon's kebab-case name.
  #[specta(optional)]
  #[serde(default)]
  pub icon: Option<String>,
  /// In the slider's range, from 0 to 1 without one; `null` hides the slider.
  #[specta(optional, type = Option<f64>)]
  #[serde(default, deserialize_with = "present")]
  pub value: Option<Option<f64>>,
  /// Makes the icon a mute button, red while `true`, which calls `onMute`; `null`
  /// makes it a plain icon again.
  #[specta(optional, type = Option<bool>)]
  #[serde(default, deserialize_with = "present")]
  pub muted: Option<Option<bool>>,
}

impl SliderState {
  fn merge(&mut self, new: SliderState) {
    let SliderState {
      title,
      icon,
      value,
      muted,
    } = new;
    self.title = title.or(self.title.take());
    self.icon = icon.or(self.icon.take());
    self.value = value.or(self.value);
    self.muted = muted.or(self.muted);
  }
}

/// A kind of dashboard item, as `onSecondary` names it
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
  Tile,
  Slider,
}

/// What the user did to one of the plugin's items, for the service
#[derive(Clone, Debug, PartialEq)]
pub enum DashboardEvent {
  Toggle(String),
  Set(String, f64),
  Mute(String),
  Secondary(Kind, String),
}

struct TileItem {
  def: TileDef,
  /// Added by the service, removed when it stops
  runtime: bool,
  /// `None` until the service sets it
  state: Option<TileState>,
}

struct SliderItem {
  def: SliderDef,
  runtime: bool,
  state: SliderState,
}

/// The items one runtime of the service added, removed when it stops
#[derive(Debug, Default, PartialEq)]
pub struct Removed {
  pub tiles: Vec<String>,
  pub sliders: Vec<String>,
}

/// One plugin's tiles and sliders and what its service set them to
pub struct PluginDashboard {
  id: String,
  tiles: BTreeMap<String, TileItem>,
  sliders: BTreeMap<String, SliderItem>,
  /// Counts the starts of the service, so a stopped run cannot stop a later one
  run: u32,
  running: bool,
}

impl EventEmitter<DashboardEvent> for PluginDashboard {}

fn icon(name: &str) -> Result<IconName> {
  icon_by_name(name).ok_or_else(|| anyhow!("unknown icon `{name}`"))
}

impl PluginDashboard {
  pub fn new(id: &str) -> Self {
    Self {
      id: id.into(),
      tiles: BTreeMap::new(),
      sliders: BTreeMap::new(),
      run: 0,
      running: false,
    }
  }

  pub fn id(&self) -> &str {
    &self.id
  }

  /// The manifest's items, in place of the ones before; the service's stay
  pub fn set_manifest(&mut self, manifest: &PluginManifest) {
    self.tiles.retain(|_, t| t.runtime);
    self.sliders.retain(|_, s| s.runtime);
    for (key, file) in &manifest.quick_actions {
      // the manifest check vouched for the icon
      let Ok(icon) = icon(&file.icon) else { continue };
      self.tiles.insert(
        key.clone(),
        TileItem {
          def: TileDef {
            title: file.name.clone(),
            icon,
          },
          runtime: false,
          state: None,
        },
      );
    }
    for (key, file) in &manifest.sliders {
      let (Ok(icon), Ok(range)) = (icon(&file.icon), Range::new(&file.range())) else {
        continue;
      };
      self.sliders.insert(
        key.clone(),
        SliderItem {
          def: SliderDef {
            title: file.name.clone(),
            icon,
            range,
            throttle: file.throttle_ms.map(|ms| Duration::from_millis(ms.into())),
          },
          runtime: false,
          state: SliderState::default(),
        },
      );
    }
  }

  /// A run of the service starts: what an earlier one added goes, what it set is reset
  pub fn start(&mut self) -> (u32, Removed) {
    self.run += 1;
    self.running = true;
    (self.run, self.reset())
  }

  /// Run `run` of the service stopped: what it added, unless a later run started
  pub fn stop(&mut self, run: u32) -> Option<Removed> {
    if run != self.run || !self.running {
      return None;
    }
    self.running = false;
    Some(self.reset())
  }

  fn reset(&mut self) -> Removed {
    let removed = Removed {
      tiles: (self.tiles.iter().filter(|(_, t)| t.runtime))
        .map(|(k, _)| k.clone())
        .collect(),
      sliders: (self.sliders.iter().filter(|(_, s)| s.runtime))
        .map(|(k, _)| k.clone())
        .collect(),
    };
    self.tiles.retain(|_, t| !t.runtime);
    self.sliders.retain(|_, s| !s.runtime);
    self.tiles.values_mut().for_each(|t| t.state = None);
    (self.sliders.values_mut()).for_each(|s| s.state = SliderState::default());
    removed
  }

  pub fn tile_keys(&self) -> impl Iterator<Item = &String> {
    self.tiles.keys()
  }

  pub fn slider_keys(&self) -> impl Iterator<Item = &String> {
    self.sliders.keys()
  }

  pub fn set_tile(&mut self, key: &str, state: TileState) -> Result<()> {
    if let Some(name) = &state.icon {
      icon(name)?;
    }
    let tile = (self.tiles.get_mut(key)).ok_or_else(|| anyhow!("no tile `{key}`"))?;
    tile.state.get_or_insert_default().merge(state);
    Ok(())
  }

  /// Adds tile `key`, or replaces one the service added before
  pub fn add_tile(&mut self, key: &str, def: TileDef, state: TileState) -> Result<()> {
    self.check_new(key, self.tiles.get(key).map(|t| t.runtime))?;
    if let Some(name) = &state.icon {
      icon(name)?;
    }
    let mut merged = TileState::default();
    merged.merge(state);
    self.tiles.insert(
      key.into(),
      TileItem {
        def,
        runtime: true,
        state: Some(merged),
      },
    );
    Ok(())
  }

  /// Whether it was there
  pub fn remove_tile(&mut self, key: &str) -> Result<bool> {
    match self.tiles.get(key).map(|t| t.runtime) {
      Some(false) => bail!("tile `{key}` is in plugin.toml, it cannot be removed"),
      Some(true) => Ok(self.tiles.remove(key).is_some()),
      None => Ok(false),
    }
  }

  pub fn set_slider(&mut self, key: &str, state: SliderState) -> Result<()> {
    if let Some(name) = &state.icon {
      icon(name)?;
    }
    let slider = (self.sliders.get_mut(key)).ok_or_else(|| anyhow!("no slider `{key}`"))?;
    slider.state.merge(state);
    Ok(())
  }

  /// Adds slider `key`, or replaces one the service added before
  pub fn add_slider(&mut self, key: &str, def: SliderDef, state: SliderState) -> Result<()> {
    self.check_new(key, self.sliders.get(key).map(|s| s.runtime))?;
    if let Some(name) = &state.icon {
      icon(name)?;
    }
    let mut merged = SliderState::default();
    merged.merge(state);
    self.sliders.insert(
      key.into(),
      SliderItem {
        def,
        runtime: true,
        state: merged,
      },
    );
    Ok(())
  }

  pub fn remove_slider(&mut self, key: &str) -> Result<bool> {
    match self.sliders.get(key).map(|s| s.runtime) {
      Some(false) => bail!("slider `{key}` is in plugin.toml, it cannot be removed"),
      Some(true) => Ok(self.sliders.remove(key).is_some()),
      None => Ok(false),
    }
  }

  /// A key the service adds is one name of `<id>:<key>`, not one of the manifest's
  fn check_new(&self, key: &str, runtime: Option<bool>) -> Result<()> {
    ensure!(
      corona_config::plugins::is_flat_name(key),
      "invalid key `{key}`: letters, digits, `.`, `_` and `-` only"
    );
    ensure!(runtime != Some(false), "`{key}` is in plugin.toml");
    Ok(())
  }

  /// The tile's title, its looks, and whether a click goes to the service
  pub fn tile(&self, key: &str) -> Option<(SharedString, IconName, SharedString, bool, bool)> {
    let tile = self.tiles.get(key)?;
    let state = tile.state.as_ref();
    let title = (state.and_then(|s| s.title.clone())).unwrap_or_else(|| tile.def.title.clone());
    let icon =
      (state.and_then(|s| s.icon.as_deref()).and_then(icon_by_name)).unwrap_or(tile.def.icon);
    let status = state.and_then(|s| s.status.clone()).unwrap_or_default();
    let active = state.and_then(|s| s.active).unwrap_or(false);
    let enabled = self.running && state.is_some_and(|s| s.enabled != Some(false));
    Some((title.into(), icon, status.into(), active, enabled))
  }

  pub fn slider_title(&self, key: &str) -> Option<SharedString> {
    let slider = self.sliders.get(key)?;
    let title = slider.state.title.clone();
    Some(title.unwrap_or_else(|| slider.def.title.clone()).into())
  }

  pub fn slider_icon(&self, key: &str) -> Option<IconName> {
    let slider = self.sliders.get(key)?;
    let icon = slider.state.icon.as_deref().and_then(icon_by_name);
    Some(icon.unwrap_or(slider.def.icon))
  }

  /// The level from 0 to 1, `None` hides the slider: no running service or no value
  pub fn slider_level(&self, key: &str) -> Option<f32> {
    let slider = self.sliders.get(key).filter(|_| self.running)?;
    let value = slider.state.value.flatten()?;
    Some(slider.def.range.normalize(value))
  }

  pub fn slider_muted(&self, key: &str) -> Option<bool> {
    let slider = self.sliders.get(key).filter(|_| self.running)?;
    slider.state.muted.flatten()
  }

  pub fn slider_def(&self, key: &str) -> Option<&SliderDef> {
    self.sliders.get(key).map(|s| &s.def)
  }

  /// Sets the slider to the value at `level`, as the dashboard shows it until the service
  /// answers, and the value the service hears of
  pub fn slide(&mut self, key: &str, level: f32) -> Option<f64> {
    let slider = self.sliders.get_mut(key)?;
    let value = slider.def.range.value(level);
    slider.state.value = Some(Some(value));
    Some(value)
  }
}

/// The dashboard id of item `key` of plugin `id`
pub fn item_id(id: &str, key: &str) -> SharedString {
  format!("{id}:{key}").into()
}

/// Calls `changed` whenever the plugin's dashboard changes
fn observe(entity: Entity<PluginDashboard>) -> corona_surface::dashboard::Observe {
  Rc::new(move |changed: Changed, cx: &mut App| vec![cx.observe(&entity, move |_, cx| changed(cx))])
}

fn emit(entity: &Entity<PluginDashboard>, event: DashboardEvent, cx: &mut App) {
  entity.update(cx, |_, cx| cx.emit(event));
}

/// The dashboard entry of tile `key`
pub fn tile_entry(entity: &Entity<PluginDashboard>, key: &str, cx: &App) -> QuickActionEntry {
  let id = item_id(entity.read(cx).id(), key);
  let (e, k) = (entity.clone(), key.to_string());
  let title =
    Rc::new(move |cx: &App| (e.read(cx).tile(&k)).map_or_else(|| k.clone().into(), |t| t.0));
  let (e, k) = (entity.clone(), key.to_string());
  let tile = Rc::new(move |cx: &App| {
    let Some((_, icon, status, active, enabled)) = e.read(cx).tile(&k) else {
      return Tile {
        icon: IconName::Puzzle,
        status: SharedString::default(),
        active: false,
        toggle: None,
      };
    };
    let (e, k) = (e.clone(), k.clone());
    Tile {
      icon,
      status,
      active,
      toggle: enabled.then(|| {
        Box::new(move |cx: &mut App| emit(&e, DashboardEvent::Toggle(k.clone()), cx)) as _
      }),
    }
  });
  let (e, k) = (entity.clone(), key.to_string());
  QuickActionEntry {
    id,
    title,
    tile,
    secondary: Some(Secondary::Run(Rc::new(move |cx| {
      emit(&e, DashboardEvent::Secondary(Kind::Tile, k.clone()), cx)
    }))),
    observe: observe(entity.clone()),
  }
}

/// The dashboard entry of slider `key`
pub fn slider_entry(entity: &Entity<PluginDashboard>, key: &str, cx: &App) -> Option<SliderEntry> {
  let dashboard = entity.read(cx);
  let def = dashboard.slider_def(key)?.clone();
  let id = item_id(dashboard.id(), key);
  fn read<R: 'static>(
    entity: &Entity<PluginDashboard>,
    key: &str,
    f: fn(&PluginDashboard, &str) -> Option<R>,
  ) -> impl Fn(&App) -> Option<R> + 'static {
    let (e, k) = (entity.clone(), key.to_string());
    move |cx: &App| f(e.read(cx), &k)
  }
  let title = read(entity, key, PluginDashboard::slider_title);
  let key_title: SharedString = key.to_string().into();
  let icon = read(entity, key, PluginDashboard::slider_icon);
  let range = def.range.clone();
  let (e, k) = (entity.clone(), key.to_string());
  let set = Rc::new(move |level: f32, cx: &mut App| {
    let value = e.update(cx, |d, cx| {
      let value = d.slide(&k, level);
      cx.notify();
      value
    });
    if let Some(value) = value {
      emit(&e, DashboardEvent::Set(k.clone(), value), cx);
    }
  });
  let (mute, secondary) = {
    let (e, k) = (entity.clone(), key.to_string());
    let (e2, k2) = (entity.clone(), key.to_string());
    (
      Rc::new(move |cx: &mut App| emit(&e, DashboardEvent::Mute(k.clone()), cx)),
      Secondary::Run(Rc::new(move |cx| {
        emit(&e2, DashboardEvent::Secondary(Kind::Slider, k2.clone()), cx)
      })),
    )
  };
  Some(SliderEntry {
    id,
    title: Rc::new(move |cx| title(cx).unwrap_or_else(|| key_title.clone())),
    throttle: def.throttle,
    step: def.range.level_step(),
    icon: Rc::new(move |cx| icon(cx).unwrap_or(IconName::Puzzle)),
    value: Rc::new(read(entity, key, PluginDashboard::slider_level)),
    label: Rc::new(move |level, _| range.label(level)),
    set,
    muted: Rc::new(read(entity, key, PluginDashboard::slider_muted)),
    toggle_mute: mute,
    secondary: Some(secondary),
    observe: observe(entity.clone()),
  })
}

/// Puts the plugin's manifest items on the dashboard
pub fn register(entity: &Entity<PluginDashboard>, manifest: &PluginManifest, cx: &mut App) {
  entity.update(cx, |d, cx| {
    d.set_manifest(manifest);
    cx.notify();
  });
  for key in manifest.quick_actions.keys() {
    let entry = tile_entry(entity, key, cx);
    cx.dashboard_mut().quick_action_entry(entry);
  }
  for key in manifest.sliders.keys() {
    if let Some(entry) = slider_entry(entity, key, cx) {
      cx.dashboard_mut().slider_entry(entry);
    }
  }
}

/// Takes the tiles and sliders the service added off the dashboard
pub fn unregister(id: &str, removed: &Removed, cx: &mut App) {
  if removed.tiles.is_empty() && removed.sliders.is_empty() {
    return;
  }
  let registry = cx.dashboard_mut();
  for key in &removed.tiles {
    registry.remove_quick_action(&item_id(id, key));
  }
  for key in &removed.sliders {
    registry.remove_slider(&item_id(id, key));
  }
}

/// Takes every item of plugin `id` off the dashboard: its tiles, sliders and cards
pub fn remove_all(id: &str, cx: &mut App) {
  let prefix = format!("{id}:");
  let registry = cx.dashboard();
  let ids = |ids: Vec<SharedString>| -> Vec<SharedString> {
    ids.into_iter().filter(|i| i.starts_with(&prefix)).collect()
  };
  let tiles = ids(
    registry
      .quick_actions()
      .iter()
      .map(|a| a.id.clone())
      .collect(),
  );
  let sliders = ids(registry.sliders().iter().map(|s| s.id.clone()).collect());
  let cards = ids(registry.cards().iter().map(|c| c.id.clone()).collect());
  if tiles.is_empty() && sliders.is_empty() && cards.is_empty() {
    return;
  }
  let registry = cx.dashboard_mut();
  tiles.iter().for_each(|id| registry.remove_quick_action(id));
  sliders.iter().for_each(|id| registry.remove_slider(id));
  cards.iter().for_each(|id| registry.remove_card(id));
}

/// The plugin's dashboard state, kept by its hub
pub fn new_entity(id: &str, cx: &mut App) -> Entity<PluginDashboard> {
  cx.new(|_: &mut Context<PluginDashboard>| PluginDashboard::new(id))
}

#[cfg(test)]
pub(crate) mod tests {
  use std::path::Path;

  use gpui_kit::{self as gpui, TestAppContext};

  use super::*;
  use crate::plugin::manifest::ManifestFile;

  pub(crate) fn manifest(rest: &str) -> PluginManifest {
    let toml = format!(
      "id = \"p\"\nname = \"P\"\napi = 2\nservice = \"s.js\"\n[capabilities]\ncorona = [\"dashboard\"]\n{rest}"
    );
    let file: ManifestFile = toml::from_str(&toml).unwrap();
    PluginManifest::new(file, "/p".into(), Path::new("/d"), None)
  }

  const ITEMS: &str = r#"
[quick_actions.lamp]
name = "Lamp"
icon = "lightbulb"
[sliders.heat]
name = "Heat"
icon = "thermometer"
min = 10
max = 30
step = 0.5
unit = "°C"
throttle_ms = 100
[sliders.level]
name = "Level"
icon = "sun"
"#;

  fn range(min: Option<f64>, max: Option<f64>, step: Option<f64>, unit: Option<&str>) -> Range {
    Range::new(&SliderRange {
      min,
      max,
      step,
      unit: unit.map(Into::into),
    })
    .unwrap()
  }

  fn def(icon: IconName) -> TileDef {
    TileDef {
      title: "T".into(),
      icon,
    }
  }

  fn slider_def() -> SliderDef {
    SliderDef {
      title: "S".into(),
      icon: IconName::Sun,
      range: range(None, None, None, None),
      throttle: None,
    }
  }

  #[test]
  fn a_range_maps_to_the_slider_and_back() {
    let heat = range(Some(10.), Some(30.), Some(0.5), Some("°C"));
    assert_eq!(heat.normalize(20.), 0.5);
    // outside the range clamps
    assert_eq!(heat.normalize(0.), 0.);
    assert_eq!(heat.normalize(99.), 1.);
    assert_eq!(heat.value(0.5), 20.);
    // snaps to the step
    assert_eq!(heat.value(0.51), 20.0);
    assert_eq!(heat.value(0.53), 20.5);
    assert_eq!(heat.value(2.), 30.);
    assert_eq!(heat.level_step(), 0.025);
    assert_eq!(heat.label(0.525), "20.5 °C");
    assert_eq!(heat.label(0.), "10 °C");

    // a hundredth of the range without a step, no unit
    let volume = range(Some(0.), Some(200.), None, None);
    assert_eq!(volume.value(0.333), 66.);
    assert_eq!(volume.label(0.333), "66");
    assert!((volume.level_step() - 0.01).abs() < 1e-6);

    // a step that does not divide evenly in binary still shows short
    let tenths = range(Some(0.), Some(1.), Some(0.1), Some(""));
    assert_eq!(tenths.label(0.3), "0.3");

    let negative = range(Some(-5.), Some(5.), Some(1.), Some("dB"));
    assert_eq!(negative.label(0.5), "0 dB");
    assert_eq!(negative.label(0.), "-5 dB");

    // no range: 0 to 1 as a percent
    let plain = range(None, None, None, None);
    assert_eq!(plain.value(0.426), 0.43);
    assert_eq!(plain.label(0.426), "43%");
    assert_eq!(plain.normalize(0.25), 0.25);
  }

  #[test]
  fn bad_ranges_are_refused() {
    let bad = |min, max, step, unit: Option<&str>| {
      Range::new(&SliderRange {
        min,
        max,
        step,
        unit: unit.map(Into::into),
      })
      .unwrap_err()
      .to_string()
    };
    assert_eq!(
      bad(Some(1.), Some(1.), None, None),
      "`min` must be below `max`"
    );
    assert_eq!(
      bad(Some(f64::NAN), Some(1.), None, None),
      "`min` must be below `max`"
    );
    assert_eq!(
      bad(Some(0.), None, None, None),
      "`min` and `max` go together"
    );
    assert_eq!(
      bad(None, None, None, Some("%")),
      "`unit` needs `min` and `max`"
    );
    assert_eq!(
      bad(Some(0.), Some(1.), Some(0.), None),
      "`step` must be above 0"
    );
    assert_eq!(bad(None, None, Some(-1.), None), "`step` must be above 0");
  }

  #[test]
  fn states_merge_and_tell_null_from_absent() {
    let mut tile = TileState::default();
    tile.merge(serde_json::from_str(r#"{"status":"On","active":true}"#).unwrap());
    tile.merge(serde_json::from_str(r#"{"title":"Lamp","active":false}"#).unwrap());
    assert_eq!(
      tile,
      TileState {
        title: Some("Lamp".into()),
        icon: None,
        status: Some("On".into()),
        active: Some(false),
        enabled: None,
      }
    );
    assert!(serde_json::from_str::<TileState>(r#"{"bogus":1}"#).is_err());

    let mut slider = SliderState::default();
    slider.merge(serde_json::from_str(r#"{"value":0.5,"muted":true}"#).unwrap());
    // left out keeps it
    slider.merge(serde_json::from_str(r#"{"title":"T"}"#).unwrap());
    assert_eq!(slider.value, Some(Some(0.5)));
    assert_eq!(slider.muted, Some(Some(true)));
    // null clears it
    slider.merge(serde_json::from_str(r#"{"value":null,"muted":null}"#).unwrap());
    assert_eq!(slider.value, Some(None));
    assert_eq!(slider.muted, Some(None));
    assert_eq!(slider.title.as_deref(), Some("T"));
  }

  #[test]
  fn manifest_items_wait_for_their_service() {
    let mut d = PluginDashboard::new("p");
    d.set_manifest(&manifest(ITEMS));
    assert_eq!(d.tile_keys().collect::<Vec<_>>(), ["lamp"]);
    assert_eq!(d.slider_keys().collect::<Vec<_>>(), ["heat", "level"]);

    // the manifest's looks, no clicks, no slider
    let (title, icon, status, active, enabled) = d.tile("lamp").unwrap();
    assert_eq!(
      (title.as_ref(), icon, status.as_ref()),
      ("Lamp", IconName::Lightbulb, "")
    );
    assert!(!active && !enabled);
    assert_eq!(d.slider_title("heat").unwrap(), "Heat");
    assert_eq!(d.slider_icon("heat").unwrap(), IconName::Thermometer);
    assert_eq!(d.slider_level("heat"), None);
    let heat = d.slider_def("heat").unwrap();
    assert_eq!(heat.throttle, Some(Duration::from_millis(100)));
    assert_eq!(heat.range.label(0.5), "20 °C");

    // set before the service runs: still disabled
    d.set_tile("lamp", TileState::default()).unwrap();
    assert!(!d.tile("lamp").unwrap().4);

    let (run, removed) = d.start();
    assert_eq!(removed, Removed::default());
    assert!(
      !d.tile("lamp").unwrap().4,
      "enabled before the service sets it"
    );
    d.set_tile(
      "lamp",
      TileState {
        title: Some("Lampe".into()),
        icon: Some("lightbulb-off".into()),
        status: Some("Aus".into()),
        active: Some(true),
        enabled: None,
      },
    )
    .unwrap();
    let (title, icon, status, active, enabled) = d.tile("lamp").unwrap();
    assert_eq!(
      (title.as_ref(), icon, status.as_ref()),
      ("Lampe", IconName::LightbulbOff, "Aus")
    );
    assert!(active && enabled);
    d.set_tile(
      "lamp",
      TileState {
        enabled: Some(false),
        ..TileState::default()
      },
    )
    .unwrap();
    assert!(!d.tile("lamp").unwrap().4);

    d.set_slider(
      "heat",
      SliderState {
        value: Some(Some(25.)),
        muted: Some(Some(false)),
        title: Some("Wärme".into()),
        icon: Some("flame".into()),
      },
    )
    .unwrap();
    assert_eq!(d.slider_icon("heat").unwrap(), IconName::Flame);
    assert_eq!(d.slider_level("heat"), Some(0.75));
    assert_eq!(d.slider_muted("heat"), Some(false));
    assert_eq!(d.slider_title("heat").unwrap(), "Wärme");
    assert_eq!(d.slide("heat", 0.26), Some(15.0));
    assert_eq!(d.slider_level("heat"), Some(0.25));
    assert_eq!(d.slide("nope", 0.5), None);

    // stopping resets what the service set
    assert_eq!(d.stop(run), Some(Removed::default()));
    let (title, _, status, active, enabled) = d.tile("lamp").unwrap();
    assert_eq!((title.as_ref(), status.as_ref()), ("Lamp", ""));
    assert!(!active && !enabled);
    assert_eq!(d.slider_level("heat"), None);
    assert_eq!(d.slider_muted("heat"), None);
    assert_eq!(d.slider_title("heat").unwrap(), "Heat");
    // twice is nothing
    assert_eq!(d.stop(run), None);
  }

  #[test]
  fn the_service_adds_and_removes_its_own() {
    let mut d = PluginDashboard::new("p");
    d.set_manifest(&manifest(ITEMS));
    let (first, _) = d.start();

    d.add_tile(
      "extra",
      def(IconName::Bell),
      TileState {
        status: Some("x".into()),
        ..TileState::default()
      },
    )
    .unwrap();
    assert!(
      d.tile("extra").unwrap().4,
      "added tiles take clicks at once"
    );
    // added again replaces
    d.add_tile("extra", def(IconName::Zap), TileState::default())
      .unwrap();
    assert_eq!(d.tile("extra").unwrap().1, IconName::Zap);
    d.add_slider(
      "vol",
      slider_def(),
      SliderState {
        value: Some(Some(0.3)),
        ..SliderState::default()
      },
    )
    .unwrap();
    assert_eq!(d.slider_level("vol"), Some(0.3));

    let err = |e: anyhow::Error| e.to_string();
    assert_eq!(
      err(
        d.add_tile("lamp", def(IconName::Bell), TileState::default())
          .unwrap_err()
      ),
      "`lamp` is in plugin.toml"
    );
    assert_eq!(
      err(
        d.add_slider("heat", slider_def(), SliderState::default())
          .unwrap_err()
      ),
      "`heat` is in plugin.toml"
    );
    assert!(
      err(
        d.add_tile("a:b", def(IconName::Bell), TileState::default())
          .unwrap_err()
      )
      .starts_with("invalid key `a:b`")
    );
    let bad_icon = TileState {
      icon: Some("no-such-icon".into()),
      ..TileState::default()
    };
    assert_eq!(
      err(
        d.add_tile("t", def(IconName::Bell), bad_icon.clone())
          .unwrap_err()
      ),
      "unknown icon `no-such-icon`"
    );
    assert_eq!(
      err(d.set_tile("lamp", bad_icon).unwrap_err()),
      "unknown icon `no-such-icon`"
    );
    assert_eq!(
      err(
        d.set_slider(
          "heat",
          SliderState {
            icon: Some("no-such-icon".into()),
            ..SliderState::default()
          }
        )
        .unwrap_err()
      ),
      "unknown icon `no-such-icon`"
    );
    assert_eq!(
      err(d.set_tile("nope", TileState::default()).unwrap_err()),
      "no tile `nope`"
    );
    assert_eq!(
      err(d.set_slider("nope", SliderState::default()).unwrap_err()),
      "no slider `nope`"
    );
    assert!(d.remove_tile("lamp").is_err());
    assert!(d.remove_slider("heat").is_err());
    assert!(!d.remove_tile("nope").unwrap());
    assert!(!d.remove_slider("nope").unwrap());

    d.add_tile("gone", def(IconName::Bell), TileState::default())
      .unwrap();
    assert!(d.remove_tile("gone").unwrap());
    assert!(d.tile("gone").is_none());

    // a restart drops what the last run added, and the old run stopping late does nothing
    let (second, removed) = d.start();
    assert_eq!(
      removed,
      Removed {
        tiles: vec!["extra".into()],
        sliders: vec!["vol".into()],
      }
    );
    assert_eq!(d.stop(first), None);
    d.add_tile("next", def(IconName::Bell), TileState::default())
      .unwrap();
    d.add_slider("s2", slider_def(), SliderState::default())
      .unwrap();
    assert!(d.remove_slider("s2").unwrap());
    d.add_slider("s3", slider_def(), SliderState::default())
      .unwrap();
    assert_eq!(
      d.stop(second),
      Some(Removed {
        tiles: vec!["next".into()],
        sliders: vec!["s3".into()],
      })
    );
    assert_eq!(d.tile_keys().collect::<Vec<_>>(), ["lamp"]);

    // a new manifest replaces the manifest's items only
    d.start();
    d.add_tile("kept", def(IconName::Bell), TileState::default())
      .unwrap();
    d.set_manifest(&manifest(""));
    assert_eq!(d.tile_keys().collect::<Vec<_>>(), ["kept"]);
    assert_eq!(d.slider_keys().count(), 0);
  }

  #[gpui::test]
  fn entries_read_the_state_and_emit_what_the_user_does(cx: &mut TestAppContext) {
    let entity = cx.update(|cx| {
      let entity = new_entity("p", cx);
      register(&entity, &manifest(ITEMS), cx);
      entity
    });
    let events = Rc::new(std::cell::RefCell::new(Vec::new()));
    let _sub = cx.update(|cx| {
      let events = events.clone();
      cx.subscribe(&entity, move |_, e: &DashboardEvent, _| {
        events.borrow_mut().push(e.clone())
      })
    });
    let (tile, heat) = cx.update(|cx| {
      let registry = cx.dashboard();
      let ids: Vec<_> = registry
        .quick_actions()
        .iter()
        .map(|a| a.id.clone())
        .collect();
      assert_eq!(ids, ["p:lamp"]);
      let ids: Vec<_> = registry.sliders().iter().map(|s| s.id.clone()).collect();
      assert_eq!(ids, ["p:heat", "p:level"]);
      (
        registry.quick_actions()[0].clone(),
        registry.sliders()[0].clone(),
      )
    });

    cx.update(|cx| {
      assert_eq!((tile.title)(cx), "Lamp");
      assert!((tile.tile)(cx).toggle.is_none());
      assert_eq!((heat.value)(cx), None);
      assert_eq!((heat.title)(cx), "Heat");
      assert_eq!((heat.icon)(cx), IconName::Thermometer);
      assert_eq!((heat.label)(0.5, cx), "20 °C");
      assert_eq!(heat.step, 0.025);
      assert_eq!(heat.throttle, Some(Duration::from_millis(100)));
    });

    // the dashboard re-renders when the state changes
    let changes = Rc::new(std::cell::Cell::new(0));
    let _subs = cx.update(|cx| {
      let changes = changes.clone();
      (tile.observe)(Rc::new(move |_| changes.set(changes.get() + 1)), cx)
    });
    cx.update(|cx| {
      entity.update(cx, |d, cx| {
        d.start();
        d.set_tile(
          "lamp",
          TileState {
            status: Some("On".into()),
            ..TileState::default()
          },
        )
        .unwrap();
        d.set_slider(
          "heat",
          SliderState {
            value: Some(Some(20.)),
            muted: Some(Some(true)),
            ..SliderState::default()
          },
        )
        .unwrap();
        cx.notify();
      })
    });
    cx.run_until_parked();
    assert!(changes.get() > 0);

    cx.update(|cx| {
      let look = (tile.tile)(cx);
      assert_eq!(look.status, "On");
      (look.toggle.expect("clickable"))(cx);
      match tile.secondary.clone().unwrap() {
        Secondary::Run(run) => run(cx),
        Secondary::Page(_) => panic!("plugins run a fn"),
      }
      assert_eq!((heat.value)(cx), Some(0.5));
      assert_eq!((heat.muted)(cx), Some(true));
      (heat.set)(0.76, cx);
      (heat.toggle_mute)(cx);
      match heat.secondary.clone().unwrap() {
        Secondary::Run(run) => run(cx),
        Secondary::Page(_) => panic!("plugins run a fn"),
      }
      // the slider shows where it was set until the service answers
      assert_eq!((heat.value)(cx), Some(0.75));
    });
    cx.run_until_parked();
    assert_eq!(
      *events.borrow(),
      [
        DashboardEvent::Toggle("lamp".into()),
        DashboardEvent::Secondary(Kind::Tile, "lamp".into()),
        DashboardEvent::Set("heat".into(), 25.),
        DashboardEvent::Mute("heat".into()),
        DashboardEvent::Secondary(Kind::Slider, "heat".into()),
      ]
    );

    // an item gone from the state shows its key
    cx.update(|cx| {
      entity.update(cx, |d, _| d.set_manifest(&manifest("")));
      assert_eq!((tile.title)(cx), "lamp");
      assert_eq!((tile.tile)(cx).icon, IconName::Puzzle);
      assert_eq!((heat.title)(cx), "heat");
      assert_eq!((heat.icon)(cx), IconName::Puzzle);
      assert!(slider_entry(&entity, "heat", cx).is_none());
    });
  }

  #[gpui::test]
  fn removing_takes_only_the_plugins_items(cx: &mut TestAppContext) {
    cx.update(|cx| {
      let entity = new_entity("p", cx);
      register(&entity, &manifest(ITEMS), cx);
      let other = new_entity("q", cx);
      entity.update(cx, |d, _| {
        d.start();
        d.add_tile("extra", def(IconName::Bell), TileState::default())
          .unwrap();
      });
      let extra = tile_entry(&entity, "extra", cx);
      let q = tile_entry(&other, "lamp", cx);
      cx.dashboard_mut()
        .quick_action_entry(extra)
        .quick_action_entry(q);
      let tiles = |cx: &App| -> Vec<String> {
        (cx.dashboard().quick_actions().iter())
          .map(|a| a.id.to_string())
          .collect()
      };
      assert_eq!(tiles(cx), ["p:lamp", "p:extra", "q:lamp"]);

      unregister("p", &Removed::default(), cx);
      unregister(
        "p",
        &Removed {
          tiles: vec!["extra".into()],
          sliders: vec!["level".into()],
        },
        cx,
      );
      assert_eq!(tiles(cx), ["p:lamp", "q:lamp"]);
      assert_eq!(cx.dashboard().sliders().len(), 1);

      remove_all("p", cx);
      assert_eq!(tiles(cx), ["q:lamp"]);
      assert!(cx.dashboard().sliders().is_empty());
      // nothing left is nothing to do
      remove_all("p", cx);
    });
  }
}
