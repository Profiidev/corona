use corona_power::PowerExt;
use gpui_kit::App;

use crate::{
  icons::power_profile,
  osds::{kinds::PowerProfileOsd, show},
};
use corona_surface::osd::{ToggleOsd, on_change};
use rust_i18n::t;

pub fn init(cx: &mut App) {
  let profiles = cx.power().profiles.clone();
  on_change(
    &profiles,
    cx,
    |cx| cx.power().profiles(cx).map(|p| p.active.clone()),
    |_, active, cx| {
      let (icon, label) = power_profile(active);
      show::<PowerProfileOsd>(
        ToggleOsd {
          icon,
          label: t!("app.power.profile.title"),
          state: label,
          active: true,
        },
        cx,
      );
    },
  );
}
