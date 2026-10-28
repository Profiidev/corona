use corona_notifications::NotificationsExt;
use gpui_kit::{App, assets::IconName};

use crate::osds::{kinds::DndOsd, show};
use corona_surface::osd::{ToggleOsd, on_change};
use rust_i18n::t;

pub fn init(cx: &mut App) {
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
      show::<DndOsd>(ToggleOsd::on_off(icon, t!("app.notifications.dnd"), on), cx);
    },
  );
}
