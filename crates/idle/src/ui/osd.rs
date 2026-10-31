use crate::IdleExt;
use gpui_kit::{App, assets::IconName};

use corona_surface::{
  osd::{ToggleOsd, toggle_on_change},
  osd_kind,
};
use rust_i18n::t;

osd_kind!(
  StayAwakeOsd(ToggleOsd),
  "stay_awake",
  "app.settings.osd.stay_awake.title",
  "app.settings.osd.stay_awake.description"
);

/// Shows stay awake turning on or off
pub(crate) fn init(cx: &mut App) {
  let stay_awake = cx.idle().stay_awake.clone();
  toggle_on_change::<StayAwakeOsd, _>(
    &stay_awake,
    cx,
    || t!("app.osd.stay_awake"),
    |_| IconName::Coffee,
    |cx| Some(cx.idle().stay_awake(cx)),
  );
}
