//! Panels of the integrations: the control center pages

use corona_surface::control_center::ControlCenterExt;
use gpui_kit::App;

pub(crate) mod bluetooth;
pub(crate) mod audio;
pub(crate) mod brightness;
pub(crate) mod calendar;
pub(crate) mod clipboard;
pub(crate) mod disks;
pub(crate) mod hotspot;
pub(crate) mod media;
pub(crate) mod network;
pub(crate) mod notifications;
pub(crate) mod power;
pub(crate) mod printers;
pub(crate) mod weather;
pub(crate) mod sysinfo;

pub(crate) fn register_all(cx: &mut App) {
  disks::init(cx);
  // the order the pages are listed in
  cx.control_center_mut()
    .register::<notifications::NotificationsPanel>()
    .register::<audio::AudioPanel>()
    .register::<clipboard::ClipboardPanel>()
    .register::<sysinfo::SysinfoPanel>()
    .register::<bluetooth::BluetoothPanel>()
    .register::<network::NetworkPanel>()
    .register::<hotspot::HotspotPanel>()
    .register::<printers::PrintersPanel>()
    .register::<power::PowerPanel>()
    .register::<disks::DisksPanel>()
    .register::<weather::WeatherPanel>()
    .register::<calendar::CalendarPanel>()
    .register::<brightness::BrightnessPanel>()
    .register::<media::MediaPanel>();
  bluetooth::open_for_pairing(cx);
}
