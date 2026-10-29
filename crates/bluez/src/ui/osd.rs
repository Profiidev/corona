use crate::BluetoothExt;
use gpui_kit::{App, assets::IconName};

use corona_surface::{
  osd::{OsdExt, ToggleOsd, on_change},
  osd_kind,
};
use corona_utils::error::ErrorLogExt;
use rust_i18n::t;

osd_kind!(
  BluetoothOsd(ToggleOsd),
  "bluetooth",
  "app.settings.osd.bluetooth.title",
  "app.settings.osd.bluetooth.description"
);

/// Shows the adapter turning on or off
pub(crate) fn init(cx: &mut App) {
  let adapter = cx.bluetooth().adapter.clone();
  on_change(
    &adapter,
    cx,
    |cx| cx.bluetooth().adapter(cx).map(|a| a.powered),
    |_, &on, cx| {
      let icon = if on {
        IconName::Bluetooth
      } else {
        IconName::BluetoothOff
      };
      let osd = ToggleOsd::on_off(icon, t!("app.bluetooth.title"), on);
      let _ = cx.show_osd(BluetoothOsd(osd)).log_err();
    },
  );
}
