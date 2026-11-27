//! What the dashboard shows, contributed by the integrations: quick action
//! tiles, sliders and cards. `control_center.{quick_actions,sliders,cards}`
//! name and order them.

use std::{rc::Rc, time::Duration};

use corona_config::{ConfigProvider, ControlCenterConfig};
use gpui_kit::{
  AnyView, App, AppContext, Context, Global, Render, SharedString, Subscription, Window,
  assets::IconName,
};

/// "On" or "Off", for a tile's status
pub fn on_off(on: bool) -> SharedString {
  match on {
    true => rust_i18n::t!("app.common.on"),
    false => rust_i18n::t!("app.common.off"),
  }
  .into()
}

mod panel;
mod sliders;
mod tiles;

pub use panel::DashboardPanel;
pub use tiles::DarkMode;

/// Opens a button of the dashboard's title bar, like the power menu
pub type OpenButton = fn(&mut Window, &mut App) -> AnyView;

/// Called when something a tile or slider shows changed
pub type Changed = Rc<dyn Fn(&mut App)>;

/// What clicking a tile does
pub type Toggle = Box<dyn Fn(&mut App)>;

/// How a quick action's tile looks now
pub struct Tile {
  pub icon: IconName,
  /// Under the title, like "On" or the connected network
  pub status: SharedString,
  pub active: bool,
  /// What a click does, `None` while it cannot be toggled
  pub toggle: Option<Toggle>,
}

pub trait QuickAction: 'static {
  /// The key in `control_center.quick_actions`
  const ID: &'static str;

  fn title() -> SharedString;
  fn tile(cx: &App) -> Tile;

  /// The control center page a right click opens
  fn page() -> Option<&'static str> {
    None
  }

  /// Subscriptions that call `changed` when the tile changes
  fn observe(_changed: Changed, _cx: &mut App) -> Vec<Subscription> {
    vec![]
  }
}

pub trait DashboardSlider: 'static {
  /// The key in `control_center.sliders`
  const ID: &'static str;
  /// Sets while dragging at most this often, the last value on release
  const THROTTLE: Option<Duration> = None;

  /// The settings' name for it
  fn title() -> SharedString;
  fn icon(cx: &App) -> IconName;
  /// The level from 0 to 1, `None` hides the slider
  fn value(cx: &App) -> Option<f32>;
  fn set(value: f32, cx: &mut App);

  /// `Some` for a slider whose icon mutes it, whether it is muted
  fn muted(_cx: &App) -> Option<bool> {
    None
  }

  fn toggle_mute(_cx: &mut App) {}

  /// The control center page the slider's chevron opens
  fn page() -> Option<&'static str> {
    None
  }

  /// Subscriptions that call `changed` when the value changes
  fn observe(_changed: Changed, _cx: &mut App) -> Vec<Subscription> {
    vec![]
  }
}

pub trait DashboardCard: Render {
  /// The key in `control_center.cards`
  const ID: &'static str;

  /// The settings' name for it
  fn title() -> SharedString;
  fn init(window: &mut Window, cx: &mut Context<Self>) -> Self;
}

/// What a tile's right click or a slider's chevron does
#[derive(Clone)]
pub enum Secondary {
  /// Opens this control center page
  Page(&'static str),
  Run(Rc<dyn Fn(&mut App)>),
}

pub type Observe = Rc<dyn Fn(Changed, &mut App) -> Vec<Subscription>>;

/// What an entry shows now
pub type Read<T> = Rc<dyn Fn(&App) -> T>;

/// A slider's text for a level from 0 to 1
pub type Label = dyn Fn(f32, &App) -> SharedString;

/// Sets a slider to a level from 0 to 1
pub type Set = dyn Fn(f32, &mut App);

/// Builds a card's view
pub type Open = Rc<dyn Fn(&mut Window, &mut App) -> AnyView>;

#[derive(Clone)]
pub struct QuickActionEntry {
  pub id: SharedString,
  pub title: Read<SharedString>,
  pub tile: Read<Tile>,
  pub secondary: Option<Secondary>,
  pub observe: Observe,
}

#[derive(Clone)]
pub struct SliderEntry {
  pub id: SharedString,
  pub title: Read<SharedString>,
  pub throttle: Option<Duration>,
  /// The slider's step on its 0 to 1 scale
  pub step: f32,
  pub icon: Read<IconName>,
  pub value: Read<Option<f32>>,
  /// The text right of the slider for a level from 0 to 1
  pub label: Rc<Label>,
  pub set: Rc<Set>,
  pub muted: Read<Option<bool>>,
  pub toggle_mute: Rc<dyn Fn(&mut App)>,
  pub secondary: Option<Secondary>,
  pub observe: Observe,
}

/// Runs what a tile's right click or a slider's chevron does
pub(crate) fn run_secondary(secondary: &Secondary, window: &mut Window, cx: &mut App) {
  match secondary {
    Secondary::Page(page) => crate::control_center::ControlCenter::navigate_id(page, window, cx),
    Secondary::Run(run) => run(cx),
  }
}

/// A level from 0 to 1 as a percent, like "42%"
pub fn percent(value: f32) -> SharedString {
  format!("{:.0}%", value * 100.).into()
}

#[derive(Clone)]
pub struct CardEntry {
  pub id: SharedString,
  pub title: Read<SharedString>,
  pub open: Open,
}

/// Everything registered for the dashboard, each list in registration order
#[derive(Default)]
pub struct DashboardRegistry {
  quick_actions: Vec<QuickActionEntry>,
  sliders: Vec<SliderEntry>,
  cards: Vec<CardEntry>,
  header: Option<fn(&mut Window, &mut App) -> AnyView>,
  buttons: Vec<OpenButton>,
}

impl Global for DashboardRegistry {}

impl DashboardRegistry {
  pub fn quick_action<A: QuickAction>(&mut self) -> &mut Self {
    self.quick_action_entry(QuickActionEntry {
      id: A::ID.into(),
      title: Rc::new(|_| A::title()),
      tile: Rc::new(A::tile),
      secondary: A::page().map(Secondary::Page),
      observe: Rc::new(A::observe),
    })
  }

  pub fn slider<S: DashboardSlider>(&mut self) -> &mut Self {
    self.slider_entry(SliderEntry {
      id: S::ID.into(),
      title: Rc::new(|_| S::title()),
      throttle: S::THROTTLE,
      step: 0.01,
      icon: Rc::new(S::icon),
      value: Rc::new(S::value),
      label: Rc::new(|value, _| percent(value)),
      set: Rc::new(S::set),
      muted: Rc::new(S::muted),
      toggle_mute: Rc::new(S::toggle_mute),
      secondary: S::page().map(Secondary::Page),
      observe: Rc::new(S::observe),
    })
  }

  pub fn card<C: DashboardCard>(&mut self) -> &mut Self {
    self.card_entry(CardEntry {
      id: C::ID.into(),
      title: Rc::new(|_| C::title()),
      open: Rc::new(|window, cx| cx.new(|cx| C::init(window, cx)).into()),
    })
  }

  /// Adds a tile, replacing one with the same id
  pub fn quick_action_entry(&mut self, entry: QuickActionEntry) -> &mut Self {
    self.remove_quick_action(&entry.id);
    self.quick_actions.push(entry);
    self
  }

  /// Adds a slider, replacing one with the same id
  pub fn slider_entry(&mut self, entry: SliderEntry) -> &mut Self {
    self.remove_slider(&entry.id);
    self.sliders.push(entry);
    self
  }

  /// Adds a card, replacing one with the same id
  pub fn card_entry(&mut self, entry: CardEntry) -> &mut Self {
    self.remove_card(&entry.id);
    self.cards.push(entry);
    self
  }

  pub fn remove_quick_action(&mut self, id: &str) {
    self.quick_actions.retain(|a| a.id != id);
  }

  pub fn remove_slider(&mut self, id: &str) {
    self.sliders.retain(|s| s.id != id);
  }

  pub fn remove_card(&mut self, id: &str) {
    self.cards.retain(|c| c.id != id);
  }

  /// The card always shown first, above the tiles, not in `control_center.cards`
  pub fn header<C: DashboardCard>(&mut self) -> &mut Self {
    self.header = Some(|window, cx| cx.new(|cx| C::init(window, cx)).into());
    self
  }

  /// Adds a button to the dashboard's title bar, after those registered before
  pub fn button(&mut self, open: OpenButton) -> &mut Self {
    self.buttons.push(open);
    self
  }

  pub fn buttons(&self) -> &[OpenButton] {
    &self.buttons
  }

  /// What opens the header card, if one registered
  pub fn header_card(&self) -> Option<fn(&mut Window, &mut App) -> AnyView> {
    self.header
  }

  pub fn quick_actions(&self) -> &[QuickActionEntry] {
    &self.quick_actions
  }

  pub fn sliders(&self) -> &[SliderEntry] {
    &self.sliders
  }

  pub fn cards(&self) -> &[CardEntry] {
    &self.cards
  }

  /// The entries `ids` name, each once, and the ids that name none or repeat one
  pub fn pick<'a, T>(
    entries: &'a [T],
    ids: &'a [String],
    id: impl Fn(&T) -> &str,
  ) -> (Vec<&'a T>, Vec<&'a str>) {
    let (found, bad) =
      crate::control_center::resolve(ids, |wanted| entries.iter().position(|e| id(e) == wanted));
    (found.into_iter().map(|i| &entries[i]).collect(), bad)
  }
}

pub trait DashboardExt {
  fn dashboard(&self) -> &DashboardRegistry;
  fn dashboard_mut(&mut self) -> &mut DashboardRegistry;
}

impl DashboardExt for App {
  fn dashboard(&self) -> &DashboardRegistry {
    thread_local! {
      static EMPTY: &'static DashboardRegistry = Box::leak(Box::default());
    }
    (self.try_global::<DashboardRegistry>()).unwrap_or_else(|| EMPTY.with(|e| *e))
  }

  fn dashboard_mut(&mut self) -> &mut DashboardRegistry {
    self.default_global::<DashboardRegistry>()
  }
}

/// Plugin items, `<plugin>:<key>`, show up once the plugin loads or its service adds
/// them, so missing ones are no mistake
fn not_plugin_items(ids: Vec<&str>) -> Vec<&str> {
  ids.into_iter().filter(|id| !id.contains(':')).collect()
}

/// Logs the quick action, slider and card ids that are skipped, now and
/// whenever they change
pub fn warn_bad_ids(cx: &mut App) {
  fn warn(config: &ControlCenterConfig, cx: &App) {
    let registry = cx.dashboard();
    let lists = [
      (
        "quick_actions",
        DashboardRegistry::pick(registry.quick_actions(), &config.quick_actions, |a| &a.id).1,
      ),
      (
        "sliders",
        DashboardRegistry::pick(registry.sliders(), &config.sliders, |s| &s.id).1,
      ),
      (
        "cards",
        DashboardRegistry::pick(registry.cards(), &config.cards, |c| &c.id).1,
      ),
    ];
    for (key, bad) in lists {
      let bad = not_plugin_items(bad);
      if !bad.is_empty() {
        tracing::warn!("control_center.{key}: skipping unknown or repeated {bad:?}");
      }
    }
  }
  warn(&cx.config().control_center, cx);
  corona_config::observe_section(cx, |c| &c.control_center, |config, cx| warn(config, cx));
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn on_off_labels() {
    assert_eq!(super::on_off(true), "On");
    assert_eq!(super::on_off(false), "Off");
  }

  struct Wifi;

  impl QuickAction for Wifi {
    const ID: &'static str = "wifi";

    fn title() -> SharedString {
      "Wi-Fi".into()
    }

    fn tile(_: &App) -> Tile {
      Tile {
        icon: IconName::Wifi,
        status: "On".into(),
        active: true,
        toggle: None,
      }
    }
  }

  struct Dnd;

  impl QuickAction for Dnd {
    const ID: &'static str = "dnd";

    fn title() -> SharedString {
      "DND".into()
    }

    fn tile(cx: &App) -> Tile {
      Wifi::tile(cx)
    }
  }

  #[test]
  fn picks_the_configured_ones_in_order() {
    let mut registry = DashboardRegistry::default();
    registry.quick_action::<Wifi>().quick_action::<Dnd>();
    // registering again replaces it
    registry.quick_action::<Wifi>();
    let ids = ["dnd", "x", "wifi", "dnd"].map(String::from);
    let (found, bad) = DashboardRegistry::pick(registry.quick_actions(), &ids, |a| &a.id);
    assert_eq!(
      found.iter().map(|a| a.id.as_ref()).collect::<Vec<_>>(),
      ["dnd", "wifi"]
    );
    assert_eq!(bad, ["x", "dnd"]);
  }

  #[test]
  fn plugin_items_are_not_warned_of() {
    assert_eq!(
      not_plugin_items(vec!["x", "demo:ping", "wifi"]),
      ["x", "wifi"]
    );
  }

  #[test]
  fn percent_rounds() {
    assert_eq!(percent(0.), "0%");
    assert_eq!(percent(0.426), "43%");
    assert_eq!(percent(1.), "100%");
  }

  #[test]
  fn traits_become_entries() {
    struct Level;
    impl DashboardSlider for Level {
      const ID: &'static str = "level";
      const THROTTLE: Option<Duration> = Some(Duration::from_millis(5));
      fn title() -> SharedString {
        "Level".into()
      }
      fn icon(_: &App) -> IconName {
        IconName::Sun
      }
      fn value(_: &App) -> Option<f32> {
        Some(0.5)
      }
      fn set(_: f32, _: &mut App) {}
      fn page() -> Option<&'static str> {
        Some("brightness")
      }
    }
    let mut registry = DashboardRegistry::default();
    registry.slider::<Level>().quick_action::<Dnd>();
    let slider = &registry.sliders()[0];
    assert_eq!((slider.id.as_ref(), slider.step), ("level", 0.01));
    assert_eq!(slider.throttle, Some(Duration::from_millis(5)));
    assert!(matches!(
      slider.secondary,
      Some(Secondary::Page("brightness"))
    ));
    assert!(registry.quick_actions()[0].secondary.is_none());
  }

  #[test]
  fn sliders_and_cards_are_replaced_and_removed() {
    struct Level;
    impl DashboardSlider for Level {
      const ID: &'static str = "level";
      fn title() -> SharedString {
        "Level".into()
      }
      fn icon(_: &App) -> IconName {
        IconName::Sun
      }
      fn value(_: &App) -> Option<f32> {
        None
      }
      fn set(_: f32, _: &mut App) {}
    }
    let mut registry = DashboardRegistry::default();
    registry.slider::<Level>();
    let mut slider = registry.sliders()[0].clone();
    slider.step = 0.5;
    registry.slider_entry(slider);
    assert_eq!(registry.sliders().len(), 1);
    assert_eq!(registry.sliders()[0].step, 0.5);
    registry.remove_slider("level");
    assert!(registry.sliders().is_empty());

    let card = || CardEntry {
      id: "p:c".into(),
      title: Rc::new(|_| "C".into()),
      open: Rc::new(|_, _| unreachable!()),
    };
    registry.card_entry(card()).card_entry(card());
    assert_eq!(registry.cards().len(), 1);
    registry.remove_card("p:c");
    assert!(registry.cards().is_empty());
  }

  #[test]
  fn entries_are_added_replaced_and_removed() {
    let mut registry = DashboardRegistry::default();
    registry.quick_action::<Wifi>();
    let mut entry = registry.quick_actions()[0].clone();
    entry.id = "plugin:a".into();
    registry.quick_action_entry(entry.clone());
    entry.secondary = Some(Secondary::Page("network"));
    registry.quick_action_entry(entry);
    let ids: Vec<_> = registry
      .quick_actions()
      .iter()
      .map(|a| a.id.clone())
      .collect();
    assert_eq!(ids, ["wifi", "plugin:a"]);
    assert!(matches!(
      registry.quick_actions()[1].secondary,
      Some(Secondary::Page("network"))
    ));
    registry.remove_quick_action("plugin:a");
    assert_eq!(registry.quick_actions().len(), 1);
  }
}
