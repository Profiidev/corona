use corona_notifications::NotificationsExt;
use gpui_kit::{App, assets::IconName};

use corona_surface::{
  osd::{ToggleOsd, toggle_on_change},
  osd_kind,
};
use rust_i18n::t;

osd_kind!(
  DndOsd(ToggleOsd),
  "dnd",
  "app.settings.osd.do_not_disturb.title",
  "app.settings.osd.do_not_disturb.description"
);

/// Shows do not disturb turning on or off
pub(crate) fn init(cx: &mut App) {
  let dnd = cx.notifications().do_not_disturb.clone();
  toggle_on_change::<DndOsd, _>(
    &dnd,
    cx,
    || t!("app.notifications.dnd"),
    |on| {
      if on {
        IconName::BellOff
      } else {
        IconName::Bell
      }
    },
    |cx| Some(cx.notifications().do_not_disturb(cx)),
  );
}
