use corona_network_manager::NetworkManagerExt;
use gpui_kit::{App, assets::IconName};

use corona_surface::{
  osd::{ToggleOsd, toggle_on_change},
  osd_kind,
};
use rust_i18n::t;

osd_kind!(
  WifiOsd(ToggleOsd),
  "wifi",
  "app.settings.osd.wi_fi.title",
  "app.settings.osd.wi_fi.description"
);

/// Shows Wi-Fi turning on or off
pub(crate) fn init(cx: &mut App) {
  let enabled = cx.network_manager().wifi_enabled.clone();
  toggle_on_change::<WifiOsd, _>(
    &enabled,
    cx,
    || t!("app.dashboard.wifi"),
    |on| {
      if on {
        IconName::Wifi
      } else {
        IconName::WifiOff
      }
    },
    |cx| {
      let nm = cx.network_manager();
      nm.wifi_supported(cx).then(|| nm.wifi_enabled(cx))
    },
  );
}
