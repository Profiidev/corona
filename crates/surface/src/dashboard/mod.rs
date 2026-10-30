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

pub struct QuickActionEntry {
  pub id: &'static str,
  pub title: fn() -> SharedString,
  pub tile: fn(&App) -> Tile,
  pub page: Option<&'static str>,
  pub observe: fn(Changed, &mut App) -> Vec<Subscription>,
}

pub struct SliderEntry {
  pub id: &'static str,
  pub title: fn() -> SharedString,
  pub throttle: Option<Duration>,
  pub icon: fn(&App) -> IconName,
  pub value: fn(&App) -> Option<f32>,
  pub set: fn(f32, &mut App),
  pub muted: fn(&App) -> Option<bool>,
  pub toggle_mute: fn(&mut App),
  pub page: Option<&'static str>,
  pub observe: fn(Changed, &mut App) -> Vec<Subscription>,
}

pub struct CardEntry {
  pub id: &'static str,
  pub title: fn() -> SharedString,
  pub open: fn(&mut Window, &mut App) -> AnyView,
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
    self.quick_actions.retain(|a| a.id != A::ID);
    self.quick_actions.push(QuickActionEntry {
      id: A::ID,
      title: A::title,
      tile: A::tile,
      page: A::page(),
      observe: A::observe,
    });
    self
  }

  pub fn slider<S: DashboardSlider>(&mut self) -> &mut Self {
    self.sliders.retain(|s| s.id != S::ID);
    self.sliders.push(SliderEntry {
      id: S::ID,
      title: S::title,
      throttle: S::THROTTLE,
      icon: S::icon,
      value: S::value,
      set: S::set,
      muted: S::muted,
      toggle_mute: S::toggle_mute,
      page: S::page(),
      observe: S::observe,
    });
    self
  }

  pub fn card<C: DashboardCard>(&mut self) -> &mut Self {
    self.cards.retain(|c| c.id != C::ID);
    self.cards.push(CardEntry {
      id: C::ID,
      title: C::title,
      open: |window, cx| cx.new(|cx| C::init(window, cx)).into(),
    });
    self
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
    id: impl Fn(&T) -> &'static str,
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
    static EMPTY: DashboardRegistry = DashboardRegistry {
      quick_actions: Vec::new(),
      sliders: Vec::new(),
      cards: Vec::new(),
      header: None,
      buttons: Vec::new(),
    };
    self.try_global::<DashboardRegistry>().unwrap_or(&EMPTY)
  }

  fn dashboard_mut(&mut self) -> &mut DashboardRegistry {
    self.default_global::<DashboardRegistry>()
  }
}

/// Logs the quick action, slider and card ids that are skipped, now and
/// whenever they change
pub fn warn_bad_ids(cx: &mut App) {
  fn warn(config: &ControlCenterConfig, cx: &App) {
    let registry = cx.dashboard();
    let lists = [
      (
        "quick_actions",
        DashboardRegistry::pick(registry.quick_actions(), &config.quick_actions, |a| a.id).1,
      ),
      (
        "sliders",
        DashboardRegistry::pick(registry.sliders(), &config.sliders, |s| s.id).1,
      ),
      (
        "cards",
        DashboardRegistry::pick(registry.cards(), &config.cards, |c| c.id).1,
      ),
    ];
    for (key, bad) in lists {
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
    let (found, bad) = DashboardRegistry::pick(registry.quick_actions(), &ids, |a| a.id);
    assert_eq!(
      found.iter().map(|a| a.id).collect::<Vec<_>>(),
      ["dnd", "wifi"]
    );
    assert_eq!(bad, ["x", "dnd"]);
  }
}
