use std::{rc::Rc, time::Duration};

use corona_config::ConfigProvider;
use corona_surface::{
  control_center::{ControlCenterPage, resolve},
  dashboard::{Changed, DashboardExt},
  panel::AppPanelExt,
};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  Anchor, AnyElement, AnyView, App, AppContext, Context, Entity, Focusable, IntoElement,
  ParentElement, Render, Styled, Subscription, Task, Window,
  assets::IconName,
  component::{button::Button, popover::Popover},
  div, px,
};
use jiff::Zoned;

use crate::control_center::dashboard::sliders::Sliders;
use corona_auth::ui::session::SessionMenu;
use corona_surface::control_center::{ControlCenter, Standalone};
use rust_i18n::t;

pub(crate) mod sliders;
pub(crate) mod toggles;

pub struct DashboardPanel {
  header: Option<AnyView>,
  sliders: Sliders,
  cards: Vec<(&'static str, AnyView)>,
  session: Entity<SessionMenu>,
  _clock: Task<()>,
  _subscriptions: Vec<Subscription>,
}

/// The tiles, sliders and cards the shell has
pub(crate) fn register(cx: &mut App) {
  toggles::register(cx);
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

    let clock = cx.spawn(async move |this, cx| {
      loop {
        let wait = 60 - u64::from(Zoned::now().second().unsigned_abs());
        cx.background_executor()
          .timer(Duration::from_secs(wait.max(1)))
          .await;
        if this.update(cx, |_, cx| cx.notify()).is_err() {
          break;
        }
      }
    });

    let header = cx.dashboard().header_card().map(|open| open(window, cx));
    let mut sliders = Sliders::new(&changed, cx);
    let cards = (cx.dashboard().cards().iter())
      .map(|c| (c.id, c.open))
      .collect::<Vec<_>>()
      .into_iter()
      .map(|(id, open)| (id, open(window, cx)))
      .collect();
    sliders.sync(window, cx);
    let session = cx.new(|cx| {
      SessionMenu::new(
        |_, cx| {
          let _ = cx.close_panel::<ControlCenter>();
          let _ = cx.close_panel::<Standalone<DashboardPanel>>();
        },
        cx,
      )
    });

    Self {
      header,
      sliders,
      cards,
      session,
      _clock: clock,
      _subscriptions: subscriptions,
    }
  }

  fn buttons(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
    vec![
      Button::new("settings")
        .icon(IconName::Settings)
        .cursor_pointer()
        .tooltip(t!("app.dashboard.settings"))
        .on_click(|_, _, cx| {
          // the dashboard is in the control center, or a panel of its own
          let _ = cx.close_panel::<ControlCenter>().log_err();
          let _ = cx.close_panel::<Standalone<DashboardPanel>>().log_err();
          let _ = corona_settings::open(None, cx).log_err();
        })
        .into_any_element(),
      Popover::new("power-menu")
        .anchor(Anchor::TopRight)
        .p_1()
        .track_focus(&self.session.focus_handle(cx))
        .on_open_change({
          let session = self.session.clone();
          move |open, _, cx| {
            if *open {
              session.update(cx, |session, cx| session.reset(cx));
            }
          }
        })
        .trigger(
          Button::new("power-menu-trigger")
            .icon(IconName::Power)
            .cursor_pointer(),
        )
        .content({
          let session = self.session.clone();
          move |_, _, _| div().w(px(SessionMenu::WIDTH)).child(session.clone())
        })
        .into_any_element(),
    ]
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
      .children(toggles::toggles(cx))
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
