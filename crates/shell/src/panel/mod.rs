//! Panels of the integrations: the control center pages

use corona_surface::control_center::ControlCenterExt;
use gpui_kit::App;

pub(crate) mod bluetooth;
pub(crate) mod sysinfo;

pub(crate) fn register_all(cx: &mut App) {
  bluetooth::register(cx);
  cx.control_center_mut().register::<sysinfo::SysinfoPanel>();
}
