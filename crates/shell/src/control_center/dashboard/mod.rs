use std::time::Duration;

use corona_bluez::BluetoothExt;
use corona_brightness::BrightnessExt;
use corona_mpris::MprisExt;
use corona_network_manager::NetworkManagerExt;
use corona_notifications::NotificationsExt;
use corona_pipewire::PipewireExt;
use corona_power::PowerExt;
use corona_surface::panel::AppPanelExt;
use corona_sysinfo::SystemMonitorExt;
use corona_utils::error::ErrorLogExt;
use corona_weather::WeatherExt;
use gpui_kit::{
  Anchor, AnyElement, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled,
  Subscription, Task, Window,
  assets::IconName,
  component::{button::Button, popover::Popover},
  div, px,
};
use jiff::Zoned;

use crate::control_center::{
  ControlCenter, ControlCenterPanel, Standalone,
  dashboard::{session::SessionMenu, sliders::Sliders},
  variants::ControlCenterType,
};

mod cards;
mod session;
pub(crate) mod sliders;
mod toggles;

pub struct DashboardPanel {
  sliders: Sliders,
  session: Entity<SessionMenu>,
  _clock: Task<()>,
  _subscriptions: Vec<Subscription>,
}

impl ControlCenterPanel for DashboardPanel {
  const TYPE: ControlCenterType = ControlCenterType::Dashboard;

  fn init(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
    macro_rules! observe {
      ($($entity:expr),* $(,)?) => {
        vec![$({
          let entity = $entity.clone();
          cx.observe(&entity, |_, _, cx| cx.notify())
        }),*]
      };
    }
    let mut subscriptions = observe![
      cx.network_manager().wifi_enabled,
      cx.network_manager().wifi_networks,
      cx.bluetooth().adapter,
      cx.bluetooth().devices,
      cx.notifications().notifications,
      cx.notifications().do_not_disturb,
      cx.power().profiles,
      cx.mpris().players,
      cx.mpris().active,
      cx.weather().weather,
      cx.system_monitor().info,
    ];

    let sink = cx.pipewire().default_sink.clone();
    let source = cx.pipewire().default_source.clone();
    let displays = cx.brightness().displays.clone();
    subscriptions.extend([
      cx.observe_in(&sink, window, |this, _, window, cx| this.resync(window, cx)),
      cx.observe_in(&source, window, |this, _, window, cx| {
        this.resync(window, cx)
      }),
      cx.observe_in(&displays, window, |this, _, window, cx| {
        this.resync(window, cx)
      }),
    ]);

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

    let mut sliders = Sliders::new(cx);
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
      session,
      _clock: clock,
      _subscriptions: subscriptions,
    }
  }

  fn buttons(&mut self, _cx: &mut Context<Self>) -> Vec<AnyElement> {
    vec![
      Button::new("settings")
        .icon(IconName::Settings)
        .cursor_pointer()
        .tooltip("Settings")
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
        .trigger(
          Button::new("power-menu-trigger")
            .icon(IconName::Power)
            .cursor_pointer(),
        )
        .content({
          let session = self.session.clone();
          move |_, _, _| {
            div()
              .w(px(SessionMenu::WIDTH))
              .h(px(SessionMenu::HEIGHT))
              .child(session.clone())
          }
        })
        .into_any_element(),
    ]
  }
}

fn open(page: ControlCenterType) -> impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut App) {
  move |_, window, cx| ControlCenter::navigate(page, window, cx)
}

impl DashboardPanel {
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
      .child(toggles::toggles(cx))
      .child(self.sliders.render(cx))
      .child(
        div()
          .flex_1()
          .flex()
          .gap_2()
          .children(cards::media(cx))
          .child(cards::clock(cx)),
      )
  }
}
