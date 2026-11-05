use std::rc::Rc;

use corona_config::ConfigProvider;
use gpui_kit::{
  AnyElement, AnyView, App, Context, IntoElement, ParentElement, Render, Styled, Subscription,
  Window, div,
};

use crate::{
  control_center::{ControlCenterPage, resolve},
  dashboard::{Changed, DashboardExt, sliders::Sliders, tiles},
};

/// The control center's first page: the header card, quick action tiles, sliders and
/// cards the integrations register
pub struct DashboardPanel {
  header: Option<AnyView>,
  pub(super) sliders: Sliders,
  cards: Vec<(&'static str, AnyView)>,
  buttons: Vec<AnyView>,
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
      .map(|a| a.observe)
      .collect();
    let subscriptions: Vec<_> = (actions.into_iter())
      .flat_map(|observe| observe(changed.clone(), cx))
      .collect();

    let header = cx.dashboard().header_card().map(|open| open(window, cx));
    let mut sliders = Sliders::new(&changed, cx);
    let cards = (cx.dashboard().cards().iter())
      .map(|c| (c.id, c.open))
      .collect::<Vec<_>>()
      .into_iter()
      .map(|(id, open)| (id, open(window, cx)))
      .collect();
    sliders.sync(window, cx);
    let buttons = (cx.dashboard().buttons().to_vec().into_iter())
      .map(|open| open(window, cx))
      .collect();
    Self {
      header,
      sliders,
      cards,
      buttons,
      _subscriptions: subscriptions,
    }
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
    let (found, _) = resolve(ids, |id| self.cards.iter().position(|(c, _)| *c == id));
    found.into_iter().map(|i| self.cards[i].1.clone()).collect()
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
