use corona_network_manager::NetworkManagerExt;
use gpui_kit::{App, assets::IconName};

use crate::osds::{kinds::WifiOsd, show};
use corona_surface::osd::{ToggleOsd, on_change};
use rust_i18n::t;

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
      show::<WifiOsd>(ToggleOsd::on_off(icon, t!("app.dashboard.wifi"), on), cx);
    },
  );
}
