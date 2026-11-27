use std::rc::Rc;

use corona_config::ConfigProvider;
use gpui_kit::{
  AnyElement, AnyView, App, Context, IntoElement, ParentElement, Render, SharedString, Styled,
  Subscription, Window, div,
};

use crate::{
  control_center::{ControlCenterPage, resolve},
  dashboard::{Changed, DashboardExt, DashboardRegistry, sliders::Sliders, tiles},
};

/// The control center's first page: the header card, quick action tiles, sliders and
/// cards the integrations register
pub struct DashboardPanel {
  header: Option<AnyView>,
  pub(super) sliders: Sliders,
  cards: Vec<(SharedString, AnyView)>,
  buttons: Vec<AnyView>,
  _registry: Subscription,
  _subscriptions: Vec<Subscription>,
}

impl ControlCenterPage for DashboardPanel {
  const ID: &'static str = "dashboard";

  fn icon() -> gpui_kit::assets::IconName {
    gpui_kit::assets::IconName::LayoutDashboard
  }

  fn title() -> gpui_kit::SharedString {
    rust_i18n::t!("app.control_center.dashboard").into()
  }

  fn init(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
    let header = cx.dashboard().header_card().map(|open| open(window, cx));
    let buttons = (cx.dashboard().buttons().to_vec().into_iter())
      .map(|open| open(window, cx))
      .collect();
    // plugins add and remove entries while the dashboard is open
    let registry = cx.observe_global_in::<DashboardRegistry>(window, Self::rebuild);
    let mut this = Self {
      header,
      sliders: Sliders::empty(),
      cards: Vec::new(),
      buttons,
      _registry: registry,
      _subscriptions: Vec::new(),
    };
    this.rebuild(window, cx);
    this
  }

  fn buttons(&mut self, _cx: &mut Context<Self>) -> Vec<AnyElement> {
    (self.buttons.iter())
      .map(|b| b.clone().into_any_element())
      .collect()
  }
}

impl DashboardPanel {
  fn configured_cards(&self, cx: &App) -> Vec<AnyView> {
    let ids = &cx.config().control_center.cards;
    let (found, _) = resolve(ids, |id| self.cards.iter().position(|(c, _)| c == id));
    found.into_iter().map(|i| self.cards[i].1.clone()).collect()
  }

  /// The tiles' subscriptions, slider rows and card views for what is registered now.
  /// Cards that stay keep their views.
  fn rebuild(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    // the tiles and sliders re-render, the sliders resync, when what they show changes
    let changed: Changed = {
      let (panel, handle) = (cx.entity().downgrade(), window.window_handle());
      Rc::new(move |cx: &mut App| {
        let panel = panel.clone();
        let _ = handle.update(cx, |_, window, cx| {
          let _ = panel.update(cx, |this, cx| this.resync(window, cx));
        });
      })
    };
    let actions: Vec<_> = (cx.dashboard().quick_actions().iter())
      .map(|a| a.observe.clone())
      .collect();
    self._subscriptions = (actions.into_iter())
      .flat_map(|observe| observe(changed.clone(), cx))
      .collect();
    self.sliders = Sliders::new(&changed, cx);
    let registered: Vec<_> = (cx.dashboard().cards().iter())
      .map(|c| (c.id.clone(), c.open.clone()))
      .collect();
    let mut old = std::mem::take(&mut self.cards);
    self.cards = (registered.into_iter())
      .map(|(id, open)| match old.iter().position(|(c, _)| *c == id) {
        Some(i) => old.swap_remove(i),
        None => (id.clone(), open(window, cx)),
      })
      .collect();
    self.sliders.sync(window, cx);
    cx.notify();
  }

  fn resync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.sliders.sync(window, cx);
    cx.notify();
  }
}

impl Render for DashboardPanel {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    div()
      .size_full()
      .flex()
      .flex_col()
      .gap_2()
      .children(self.header.clone())
      .children(tiles::tiles(cx))
      .child(self.sliders.render(cx))
      .child(
        div()
          .flex_1()
          .flex()
          .gap_2()
          .children(self.configured_cards(cx)),
      )
  }
}

#[cfg(test)]
mod tests {
  use std::cell::Cell;

  use gpui_kit::{
    self as gpui, AnyWindowHandle, AppContext, Entity, InteractiveElement, TestAppContext,
    WindowOptions,
    assets::IconName,
    test::{TestSupportExt, TestWindowExt},
  };

  use super::*;
  use crate::dashboard::{
    CardEntry, DashboardRegistry, QuickActionEntry, Secondary, SliderEntry, Tile, percent,
  };

  struct Card;

  impl Render for Card {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
      div().id("plugin-card").test_support().size_4()
    }
  }

  fn tile(id: &str, level: Entity<f32>) -> QuickActionEntry {
    QuickActionEntry {
      id: id.to_string().into(),
      title: Rc::new(|_| "Tile".into()),
      tile: Rc::new(|_| Tile {
        icon: IconName::Puzzle,
        status: "On".into(),
        active: true,
        toggle: Some(Box::new(|_| {})),
      }),
      secondary: Some(Secondary::Run(Rc::new(|_| {}))),
      observe: Rc::new(move |changed, cx| vec![cx.observe(&level, move |_, cx| changed(cx))]),
    }
  }

  fn slider(id: &str, level: Entity<f32>) -> SliderEntry {
    let read = level.clone();
    SliderEntry {
      id: id.to_string().into(),
      title: Rc::new(|_| "Slider".into()),
      throttle: None,
      step: 0.1,
      icon: Rc::new(|_| IconName::Sun),
      value: Rc::new(move |cx| Some(*read.read(cx))),
      label: Rc::new(|level, _| percent(level)),
      set: Rc::new(|_, _| {}),
      muted: Rc::new(|_| None),
      toggle_mute: Rc::new(|_| {}),
      secondary: None,
      observe: Rc::new(|_, _| vec![]),
    }
  }

  fn open(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<DashboardPanel>) {
    cx.update(|cx| {
      gpui_kit::init(cx);
      let mut config = corona_config::Config::default();
      config.control_center.quick_actions = vec!["p:tile".into()];
      config.control_center.sliders = vec!["p:slider".into()];
      config.control_center.cards = vec!["p:card".into()];
      cx.set_global(config);
      let mut panel = None;
      let window = gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
        let view = cx.new(|cx| DashboardPanel::init(window, cx));
        panel = Some(view.clone());
        view
      })
      .unwrap()
      .0;
      (window, panel.unwrap())
    })
  }

  fn has(window: AnyWindowHandle, id: &'static str, cx: &mut TestAppContext) -> bool {
    cx.update_window(window, |_, window, cx| {
      window.render_frame(cx);
      window.try_find(id).is_some()
    })
    .unwrap()
  }

  #[gpui::test]
  fn follows_entries_added_and_removed_while_open(cx: &mut TestAppContext) {
    let (window, panel) = open(cx);
    assert!(!has(window, "toggle-p:tile", cx));
    assert!(!has(window, "dashboard-p:slider-icon", cx));
    assert!(!has(window, "plugin-card", cx));

    let level = cx.update(|cx| cx.new(|_| 0.5_f32));
    let opened = Rc::new(Cell::new(0));
    cx.update(|cx| {
      let opened = opened.clone();
      cx.dashboard_mut()
        .quick_action_entry(tile("p:tile", level.clone()))
        .slider_entry(slider("p:slider", level.clone()))
        .card_entry(CardEntry {
          id: "p:card".into(),
          title: Rc::new(|_| "Card".into()),
          open: Rc::new(move |_, cx| {
            opened.set(opened.get() + 1);
            cx.new(|_| Card).into()
          }),
        });
    });
    cx.run_until_parked();
    assert!(has(window, "toggle-p:tile", cx));
    assert!(has(window, "dashboard-p:slider-icon", cx));
    assert!(has(window, "plugin-card", cx));
    assert_eq!(opened.get(), 1);

    // another change keeps the card's view
    cx.update(|cx| {
      cx.dashboard_mut().remove_quick_action("p:tile");
    });
    cx.run_until_parked();
    assert!(!has(window, "toggle-p:tile", cx));
    assert!(has(window, "plugin-card", cx));
    assert_eq!(opened.get(), 1);

    // the slider follows what it observes
    cx.update(|cx| {
      level.update(cx, |l, cx| {
        *l = 0.8;
        cx.notify();
      })
    });
    cx.run_until_parked();
    let shown = cx.update(|cx| {
      panel.read(cx).sliders.rows[0]
        .slider
        .read(cx)
        .shown(0.8, cx)
    });
    assert!((shown - 0.8).abs() < 0.01, "{shown}");

    cx.update(|cx| {
      let registry: &mut DashboardRegistry = cx.dashboard_mut();
      registry.remove_slider("p:slider");
      registry.remove_card("p:card");
    });
    cx.run_until_parked();
    assert!(!has(window, "dashboard-p:slider-icon", cx));
    assert!(!has(window, "plugin-card", cx));
    assert!(cx.update(|cx| panel.read(cx).cards.is_empty()));
  }

  #[gpui::test]
  fn secondary_runs_its_fn(cx: &mut TestAppContext) {
    let (window, _) = open(cx);
    let ran = Rc::new(Cell::new(false));
    let secondary = Secondary::Run(Rc::new({
      let ran = ran.clone();
      move |_| ran.set(true)
    }));
    cx.update_window(window, |_, window, cx| {
      crate::dashboard::run_secondary(&secondary, window, cx)
    })
    .unwrap();
    assert!(ran.get());
  }
}
