use crate::PowerExt;
use gpui_kit::App;

use corona_surface::{
  osd::{OsdExt, ToggleOsd, on_change},
  osd_kind,
};
use corona_utils::error::ErrorLogExt;

use crate::ui::icons::power_profile;

osd_kind!(
  PowerProfileOsd(ToggleOsd),
  "power_profile",
  "app.settings.osd.power_profile.title",
  "app.settings.osd.power_profile.description"
);
use rust_i18n::t;

/// Shows the profile switched to
pub(crate) fn init(cx: &mut App) {
  cx.osd_mut().register::<PowerProfileOsd>();
  let profiles = cx.power().profiles.clone();
  on_change(
    &profiles,
    cx,
    |cx| cx.power().profiles(cx).map(|p| p.active.clone()),
    |_, active, cx| {
      let (icon, label) = power_profile(active);
      let osd = ToggleOsd {
        icon,
        label: t!("app.power.profile.title"),
        state: label,
        active: true,
      };
      let _ = cx.show_osd(PowerProfileOsd(osd)).log_err();
    },
  );
}
