//! Bar widgets of the integrations

use corona_surface::bar::{BarExt, PageButton};
use gpui_kit::App;

mod bluetooth;
pub(crate) mod sysinfo;

pub(crate) fn register_all(cx: &mut App) {
  cx.bar_mut()
    .register::<bluetooth::BluetoothButton>()
    .register::<PageButton<crate::panel::sysinfo::SysinfoPanel>>()
    .register::<sysinfo::Resource>();
}
