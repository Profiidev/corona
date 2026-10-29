use crate::NetworkManagerExt;
use gpui_kit::{App, assets::IconName};

use corona_surface::{
  osd::{OsdExt, ToggleOsd, on_change},
  osd_kind,
};
use corona_utils::error::ErrorLogExt;

osd_kind!(
  WifiOsd(ToggleOsd),
  "wifi",
  "app.settings.osd.wi_fi.title",
  "app.settings.osd.wi_fi.description"
);
use rust_i18n::t;

/// Shows Wi-Fi turning on or off
pub(crate) fn init(cx: &mut App) {
  cx.osd_mut().register::<WifiOsd>();
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
      let osd = ToggleOsd::on_off(icon, t!("app.dashboard.wifi"), on);
      let _ = cx.show_osd(WifiOsd(osd)).log_err();
    },
  );
}
