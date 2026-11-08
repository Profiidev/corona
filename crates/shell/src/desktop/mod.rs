//! The taskbar, window switcher and screen corners

use gpui_kit::App;

pub(crate) mod screen_corners;
pub(crate) mod switcher;
pub(crate) mod taskbar;

pub(crate) fn register_all(cx: &mut App) {
  switcher::init(cx);
  taskbar::init(cx);
  screen_corners::init(cx);
}
