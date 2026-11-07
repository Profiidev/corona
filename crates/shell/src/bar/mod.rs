//! Bar widgets of the integrations

use corona_surface::bar::BarExt;
use gpui_kit::App;

mod bluetooth;

pub(crate) fn register_all(cx: &mut App) {
  cx.bar_mut().register::<bluetooth::BluetoothButton>();
}
