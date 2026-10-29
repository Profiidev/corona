use crate::IdleExt;
use gpui_kit::{App, assets::IconName};

use corona_surface::{
  osd::{OsdExt, ToggleOsd, on_change},
  osd_kind,
};
use corona_utils::error::ErrorLogExt;

osd_kind!(
  StayAwakeOsd(ToggleOsd),
  "stay_awake",
  "app.settings.osd.stay_awake.title",
  "app.settings.osd.stay_awake.description"
);
use rust_i18n::t;

/// Shows stay awake turning on or off
pub(crate) fn init(cx: &mut App) {
  cx.osd_mut().register::<StayAwakeOsd>();
  let stay_awake = cx.idle().stay_awake.clone();
  on_change(
    &stay_awake,
    cx,
    |cx| Some(cx.idle().stay_awake(cx)),
    |_, &on, cx| {
      let osd = ToggleOsd::on_off(IconName::Coffee, t!("app.osd.stay_awake"), on);
      let _ = cx.show_osd(StayAwakeOsd(osd)).log_err();
    },
  );
}
