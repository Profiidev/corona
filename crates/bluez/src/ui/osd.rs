use crate::BluetoothExt;
use gpui_kit::{App, assets::IconName};

use corona_surface::{
  osd::{ToggleOsd, toggle_on_change},
  osd_kind,
};
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
  toggle_on_change::<BluetoothOsd, _>(
    &adapter,
    cx,
    || t!("app.bluetooth.title"),
    |on| {
      if on {
        IconName::Bluetooth
      } else {
        IconName::BluetoothOff
      }
    },
    |cx| cx.bluetooth().adapter(cx).map(|a| a.powered),
  );
}
