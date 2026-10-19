use corona_idle::IdleExt;
use gpui_kit::{App, assets::IconName};

use crate::osds::{
  on_change,
  view::{ToggleOsd, show},
};
use rust_i18n::t;

pub fn init(cx: &mut App) {
  let stay_awake = cx.idle().stay_awake.clone();
  on_change(
    &stay_awake,
    cx,
    |cx| Some(cx.idle().stay_awake(cx)),
    |_, &on, cx| {
      show(
        |k| k.stay_awake,
        ToggleOsd::on_off(IconName::Coffee, t!("app.osd.stay_awake"), on),
        cx,
      );
    },
  );
}
