use corona_bluez::BluetoothExt;
use gpui_kit::{App, assets::IconName};

use crate::osds::{kinds::BluetoothOsd, show};
use corona_surface::osd::{ToggleOsd, on_change};
use rust_i18n::t;

pub fn init(cx: &mut App) {
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
      show::<BluetoothOsd>(ToggleOsd::on_off(icon, t!("app.bluetooth.title"), on), cx);
    },
  );
}
