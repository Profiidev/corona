//! The disks page with its job history and drive notifications, its bar button and the
//! drive password prompt

use corona_surface::{bar::BarExt, control_center::ControlCenterExt};
use gpui_kit::App;

pub mod drive_password;
pub mod page;
pub mod widget;

pub(crate) fn register(cx: &mut App) {
  cx.bar_mut().register::<widget::DisksButton>();
  cx.control_center_mut().register::<page::DisksPanel>();
  page::init(cx);
  drive_password::init(cx);
}
