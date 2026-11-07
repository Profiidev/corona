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
  bluetooth::register(cx);
  cx.control_center_mut()
    .register::<sysinfo::SysinfoPanel>()
    .register::<printers::PrintersPanel>()
    .register::<media::MediaPanel>()
    .register::<notifications::NotificationsPanel>()
    .register::<clipboard::ClipboardPanel>()
    .register::<audio::AudioPanel>()
    .register::<weather::WeatherPanel>()
    .register::<calendar::CalendarPanel>()
    .register::<disks::DisksPanel>()
    .register::<network::NetworkPanel>()
    .register::<hotspot::HotspotPanel>()
    .register::<brightness::BrightnessPanel>()
    .register::<power::PowerPanel>();
}
