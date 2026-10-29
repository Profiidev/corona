use crate::NotificationsExt;
use gpui_kit::{App, assets::IconName};

use corona_surface::{
  osd::{OsdExt, ToggleOsd, on_change},
  osd_kind,
};
use corona_utils::error::ErrorLogExt;
use rust_i18n::t;

osd_kind!(
  DndOsd(ToggleOsd),
  "dnd",
  "app.settings.osd.do_not_disturb.title",
  "app.settings.osd.do_not_disturb.description"
);

/// Shows do not disturb turning on or off
pub(crate) fn init(cx: &mut App) {
  cx.osd_mut().register::<DndOsd>();
  let dnd = cx.notifications().do_not_disturb.clone();
  on_change(
    &dnd,
    cx,
    |cx| Some(cx.notifications().do_not_disturb(cx)),
    |_, &on, cx| {
      let icon = if on {
        IconName::BellOff
      } else {
        IconName::Bell
      };
      let osd = ToggleOsd::on_off(icon, t!("app.notifications.dnd"), on);
      let _ = cx.show_osd(DndOsd(osd)).log_err();
    },
  );
}
