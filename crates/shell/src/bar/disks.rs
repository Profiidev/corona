//! The disks bar button

use crate::panel::disks::{DisksPanel, jobs::log as disks_log};
use corona_surface::bar::{IdlePage, IdlePageButton};
use corona_udisks::UdisksExt;
use gpui_kit::{App, Context, Subscription};

/// Busy while a job runs or a removable drive with media is in
impl IdlePage for DisksPanel {
  fn busy(cx: &App) -> bool {
    let udisks = cx.udisks();
    let removable = udisks
      .list_drives(cx)
      .iter()
      .any(|d| d.is_removable() && d.media_available);
    removable || disks_log(cx).running_count(cx) > 0
  }

  fn observe(cx: &mut Context<IdlePageButton<Self>>) -> Vec<Subscription> {
    let udisks = cx.udisks().clone();
    let log = disks_log(cx).clone();
    vec![
      cx.observe(&udisks.drives, |_, _, cx| cx.notify()),
      cx.observe(&udisks.jobs, |_, _, cx| cx.notify()),
      cx.observe(&log.running, |_, _, cx| cx.notify()),
    ]
  }
}

#[cfg(test)]
mod tests {
  use corona_udisks::Udisks;
  use corona_utils::test_bus::TestBus;
  use gpui_kit::{self as gpui, AppContext, TestAppContext, WindowOptions, test::TestWindowExt};

  use super::*;
  use crate::panel::disks::{init as init_disks, test_drive as disks_test_drive};
  use corona_surface::bar::{IdleOptions, Widget};
  use uuid::Uuid;

  fn shown(cx: &mut TestAppContext, hide_when_idle: bool, removable: bool) -> bool {
    shown_drive(cx, hide_when_idle, disks_test_drive(removable))
  }

  fn shown_drive(
    cx: &mut TestAppContext,
    hide_when_idle: bool,
    drive: corona_udisks::Drive,
  ) -> bool {
    cx.executor().allow_parking();
    let (_bus, conn) = TestBus::with_conn();
    cx.update(|cx| {
      gpui_kit::init(cx);
      cx.set_global(corona_config::Config::default());
      corona_surface::bar::BarState::init(cx);
      Udisks::fake(cx, &conn, vec![drive], vec![], vec![]);
      init_disks(cx);
    });
    let (window, _) = cx
      .update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
          cx.new(|cx| {
            IdlePageButton::<DisksPanel>::init(cx, Uuid::nil(), IdleOptions { hide_when_idle })
          })
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

  #[gpui::test]
  fn an_empty_card_reader_stays_idle(cx: &mut TestAppContext) {
    let mut drive = disks_test_drive(true);
    drive.media_available = false;
    assert!(!shown_drive(cx, true, drive));
  }
}
