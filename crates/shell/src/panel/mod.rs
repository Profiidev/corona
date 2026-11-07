//! Panels of the integrations: the control center pages

use corona_surface::control_center::ControlCenterExt;
use gpui_kit::App;

pub(crate) mod bluetooth;
pub(crate) mod audio;
pub(crate) mod clipboard;
pub(crate) mod media;
pub(crate) mod notifications;
pub(crate) mod printers;
pub(crate) mod sysinfo;

pub(crate) fn register_all(cx: &mut App) {
  bluetooth::register(cx);
  cx.control_center_mut()
    .register::<sysinfo::SysinfoPanel>()
    .register::<printers::PrintersPanel>()
    .register::<media::MediaPanel>()
    .register::<notifications::NotificationsPanel>()
    .register::<clipboard::ClipboardPanel>()
    .register::<audio::AudioPanel>();
}
