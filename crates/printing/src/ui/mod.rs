//! The printers page and its bar button

use corona_surface::{bar::BarExt, control_center::ControlCenterExt};
use gpui_kit::App;

pub mod page;
pub mod widget;

pub(crate) fn register(cx: &mut App) {
  cx.bar_mut().register::<widget::PrintersButton>();
  cx.control_center_mut().register::<page::PrintersPanel>();
}
