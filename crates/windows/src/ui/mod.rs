//! Workspaces, the active window, the switcher, the taskbar, screen corners, the displays
//! page and the keyboard OSDs

use corona_surface::{
  bar::{BarExt, PageButton},
  control_center::ControlCenterExt,
};
use gpui_kit::App;

mod active_window;
pub mod displays;
pub mod osd;
pub mod screen_corners;
pub mod switcher;
pub mod taskbar;
pub mod workspaces;

pub(crate) fn register(cx: &mut App) {
  cx.bar_mut()
    .register::<PageButton<displays::DisplaysPanel>>()
    .register::<workspaces::Workspaces>()
    .register::<active_window::ActiveWindow>();
  cx.control_center_mut()
    .register::<displays::DisplaysPanel>();
  osd::init(cx);
  switcher::init(cx);
  taskbar::init(cx);
  screen_corners::init(cx);
}
