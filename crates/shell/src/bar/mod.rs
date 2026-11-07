//! Bar widgets of the integrations

use corona_surface::bar::{BarExt, IdlePageButton, PageButton};
use gpui_kit::App;

pub(crate) mod audio;
mod bluetooth;
mod disks;
mod hotspot;
mod network;
pub(crate) mod privacy;
pub(crate) mod tray;
mod notifications;
pub(crate) mod mpris;
mod power;
mod printing;
pub(crate) mod sysinfo;

pub(crate) fn register_all(cx: &mut App) {
  privacy::init_filter(cx);
  cx.bar_mut()
    .register::<bluetooth::BluetoothButton>()
    .register::<PageButton<crate::panel::sysinfo::SysinfoPanel>>()
    .register::<sysinfo::Resource>()
    .register::<PageButton<crate::panel::media::MediaPanel>>()
    .register::<mpris::ActivePlayer>()
    .register::<notifications::NotificationsButton>()
    .register::<tray::Tray>()
    .register::<audio::AudioButton>()
    .register::<IdlePageButton<crate::panel::disks::DisksPanel>>()
    .register::<network::NetworkButton>()
    .register::<hotspot::HotspotButton>()
    .register::<PageButton<crate::panel::brightness::BrightnessPanel>>()
    .register::<PageButton<crate::panel::power::PowerPanel>>()
    .register::<power::BatteryButton>()
    .register::<privacy::Privacy>()
    .register::<PageButton<crate::panel::clipboard::ClipboardPanel>>()
    .register::<IdlePageButton<crate::panel::printers::PrintersPanel>>();
}
