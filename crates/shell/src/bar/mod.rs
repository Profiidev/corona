//! Bar widgets of the integrations

use corona_surface::bar::{BarExt, IdlePageButton, PageButton};
use gpui_kit::App;

mod bluetooth;
mod notifications;
pub(crate) mod mpris;
mod printing;
pub(crate) mod sysinfo;

pub(crate) fn register_all(cx: &mut App) {
  cx.bar_mut()
    .register::<bluetooth::BluetoothButton>()
    .register::<PageButton<crate::panel::sysinfo::SysinfoPanel>>()
    .register::<sysinfo::Resource>()
    .register::<PageButton<crate::panel::media::MediaPanel>>()
    .register::<mpris::ActivePlayer>()
    .register::<notifications::NotificationsButton>()
    .register::<IdlePageButton<crate::panel::printers::PrintersPanel>>();
}
