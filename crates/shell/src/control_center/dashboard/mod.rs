use std::{rc::Rc, time::Duration};

use corona_config::ConfigProvider;
use corona_surface::{
  control_center::{ControlCenterPage, resolve},
  dashboard::{Changed, DashboardExt},
  panel::AppPanelExt,
};
use corona_sysinfo::SystemMonitorExt;
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  Anchor, AnyElement, AnyView, App, AppContext, Context, Entity, Focusable, IntoElement,
  ParentElement, Render, Styled, Subscription, Task, Window,
  assets::IconName,
  component::{button::Button, popover::Popover},
  div, px,
};
use jiff::Zoned;

use crate::control_center::dashboard::{session::SessionMenu, sliders::Sliders};
use corona_surface::control_center::{ControlCenter, Standalone};
use rust_i18n::t;

pub(crate) mod cards;
mod session;
pub(crate) mod sliders;
pub(crate) mod toggles;

pub struct DashboardPanel {
  sliders: Sliders,
  cards: Vec<(&'static str, AnyView)>,
  session: Entity<SessionMenu>,
  _clock: Task<()>,
  _subscriptions: Vec<Subscription>,
}

/// The tiles, sliders and cards the shell has
pub(crate) fn register(cx: &mut App) {
  toggles::register(cx);
  sliders::register(cx);
  cards::register(cx);
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
    let mut subscriptions: Vec<_> = (actions.into_iter())
      .flat_map(|observe| observe(changed.clone(), cx))
      .collect();
    // the profile card
    let info = cx.system_monitor().info.clone();
    subscriptions.push(cx.observe(&info, |_, _, cx| cx.notify()));

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
          let _ = crate::settings::open(None, cx).log_err();
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

fn open(page: &'static str) -> impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut App) {
  move |_, window, cx| ControlCenter::navigate_id(page, window, cx)
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

fn spawn_logged(cx: &mut App, task: impl Future<Output = anyhow::Result<()>> + 'static) {
  cx.spawn(async move |_| {
    let _ = task.await.log_err();
  })
  .detach();
}

impl Render for DashboardPanel {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    div()
      .size_full()
      .flex()
      .flex_col()
      .gap_2()
      .child(cards::profile(cx))
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
