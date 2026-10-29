//! Networks, VPNs, the hotspot, Wi-Fi and airplane mode toggles, and IP addresses as
//! launcher commands

use gpui_kit::App;

mod ip;
mod network;

pub(crate) fn register(cx: &mut App) {
  network::register(cx);
  ip::register(cx);
}
