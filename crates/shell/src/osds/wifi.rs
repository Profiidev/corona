use corona_network_manager::NetworkManagerExt;
use gpui_kit::{App, assets::IconName};

use crate::osds::{
  on_change,
  view::{ToggleOsd, show},
};

pub fn init(cx: &mut App) {
  let enabled = cx.network_manager().wifi_enabled.clone();
  on_change(
    &enabled,
    cx,
    |cx| {
      let nm = cx.network_manager();
      nm.wifi_supported(cx).then(|| nm.wifi_enabled(cx))
    },
    |_, &on, cx| {
      let icon = if on {
        IconName::Wifi
      } else {
        IconName::WifiOff
      };
      show(ToggleOsd::on_off(icon, "Wi-Fi", on), cx);
    },
  );
}
