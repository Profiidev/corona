use corona_bluez::BluetoothExt;
use gpui_kit::{App, assets::IconName};

use crate::osds::{
  on_change,
  view::{ToggleOsd, show},
};

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
      show(ToggleOsd::on_off(icon, "Bluetooth", on), cx);
    },
  );
}
