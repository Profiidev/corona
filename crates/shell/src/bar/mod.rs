//! Bar widgets of the integrations

use corona_surface::bar::{BarExt, IdlePageButton, PageButton};
use gpui_kit::App;

mod active_window;
pub(crate) mod audio;
mod bluetooth;
pub(crate) mod clock;
mod disks;
mod hotspot;
pub(crate) mod mpris;
mod network;
mod notifications;
mod power;
mod printing;
pub(crate) mod privacy;
pub(crate) mod sysinfo;
pub(crate) mod tray;
mod workspaces;

pub(crate) fn register_all(cx: &mut App) {
  privacy::init_filter(cx);
  cx.bar_mut()
    .register::<PageButton<crate::panel::displays::DisplaysPanel>>()
    .register::<workspaces::Workspaces>()
    .register::<active_window::ActiveWindow>()
    .register::<bluetooth::BluetoothButton>()
    .register::<PageButton<crate::panel::sysinfo::SysinfoPanel>>()
    .register::<sysinfo::Resource>()
    .register::<PageButton<crate::panel::media::MediaPanel>>()
    .register::<mpris::ActivePlayer>()
    .register::<notifications::NotificationsButton>()
    .register::<tray::Tray>()
    .register::<audio::AudioButton>()
    .register::<PageButton<crate::panel::weather::WeatherPanel>>()
    .register::<PageButton<crate::panel::calendar::CalendarPanel>>()
    .register::<clock::Clock>()
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
