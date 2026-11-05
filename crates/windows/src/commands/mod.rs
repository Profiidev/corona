//! `window:*`, `monitor:*`, `dpms:*`, `lock_key:*` and `switcher:*` IPC commands

use gpui_kit::App;

pub mod dpms;
pub mod lock_key;
pub mod monitor;
pub mod switcher;
pub mod window;

pub(crate) fn register(cx: &mut App) {
  dpms::register(cx);
  lock_key::register(cx);
  monitor::register(cx);
  switcher::register(cx);
  window::register(cx);
}
