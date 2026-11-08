//! Networks, VPNs, the hotspot, Wi-Fi and airplane mode toggles, and IP addresses as
//! launcher commands

use gpui_kit::App;

mod ip;
mod networks;

pub(crate) fn register(cx: &mut App) {
  networks::register(cx);
  ip::register(cx);
}
