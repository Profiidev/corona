use corona_surface::bar::{Button, Widget};
use corona_udisks::UdisksExt;
use gpui_kit::{App, Context, IntoElement, Render, Subscription, Window, assets::IconName, div};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::control_center::{DisksPanel, Standalone, disks_log};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
  /// only show while a disk job runs or a removable drive is attached
  pub hide_when_idle: bool,
}

pub struct DisksButton {
  options: Options,
  _subscriptions: Vec<Subscription>,
}

/// A job runs or a removable drive is in
fn busy(cx: &App) -> bool {
  let udisks = cx.udisks();
  let removable = udisks.list_drives(cx).iter().any(|d| d.is_removable());
  removable || disks_log(cx).running_count(cx) > 0
}

impl Widget for DisksButton {
  const NAME: &'static str = "disks";
  type Options = Options;

  fn init(cx: &mut Context<'_, Self>, _display_id: Uuid, options: Options) -> Self {
    let udisks = cx.udisks().clone();
    let log = disks_log(cx).clone();
    Self {
      options,
      _subscriptions: vec![
        cx.observe(&udisks.drives, |_, _, cx| cx.notify()),
        cx.observe(&udisks.jobs, |_, _, cx| cx.notify()),
        cx.observe(&log.running, |_, _, cx| cx.notify()),
      ],
    }
  }
}

impl Render for DisksButton {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    if self.options.hide_when_idle && !busy(cx) {
      return div().into_any_element();
    }
    Button::<_, Standalone<DisksPanel>>::new(cx, "disks-button", IconName::HardDrive)
      .into_any_element()
  }
}

#[cfg(test)]
mod tests {
  use corona_udisks::Udisks;
  use corona_utils::test_bus::TestBus;
  use futures::executor::block_on;
  use gpui_kit::{self as gpui, AppContext, TestAppContext, WindowOptions, test::TestWindowExt};

  use super::*;
  use crate::control_center::disks_test_drive;

  fn shown(cx: &mut TestAppContext, hide_when_idle: bool, removable: bool) -> bool {
    cx.executor().allow_parking();
    let bus = TestBus::new();
    let conn = block_on(bus.conn());
    cx.update(|cx| {
      gpui_kit::init(cx);
      cx.set_global(corona_config::Config::default());
      corona_surface::bar::BarState::init(cx);
      Udisks::fake(cx, &conn, vec![disks_test_drive(removable)], vec![], vec![]);
      crate::control_center::init_disks(cx);
    });
    let (window, _) = cx
      .update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
          cx.new(|cx| DisksButton::init(cx, Uuid::nil(), Options { hide_when_idle }))
        })
      })
      .unwrap();
    cx.update_window(window, |_, window, cx| {
      window.render_frame(cx);
      window.try_find("disks-button").is_some()
    })
    .unwrap()
  }

  #[gpui::test]
  fn hides_only_when_asked_and_idle(cx: &mut TestAppContext) {
    assert!(shown(cx, false, false));
  }

  #[gpui::test]
  fn idle_without_removable_drives(cx: &mut TestAppContext) {
    assert!(!shown(cx, true, false));
  }

  #[gpui::test]
  fn a_usb_stick_shows_it(cx: &mut TestAppContext) {
    assert!(shown(cx, true, true));
  }
}
