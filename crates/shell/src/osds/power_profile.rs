use corona_power::PowerExt;
use gpui_kit::App;

use crate::{
  icons::power_profile,
  osds::{
    on_change,
    view::{ToggleOsd, show},
  },
};

pub fn init(cx: &mut App) {
  let profiles = cx.power().profiles.clone();
  on_change(
    &profiles,
    cx,
    |cx| cx.power().profiles(cx).map(|p| p.active.clone()),
    |_, active, cx| {
      let (icon, label) = power_profile(active);
      show(
        |k| k.power_profile,
        ToggleOsd {
          icon,
          label: "Power profile".into(),
          state: label.into(),
          active: true,
        },
        cx,
      );
    },
  );
}
