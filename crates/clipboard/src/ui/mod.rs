//! The clipboard page and its bar button

use corona_surface::{
  bar::{BarExt, PageButton},
  control_center::ControlCenterExt,
};
use gpui_kit::App;

pub mod page;

pub(crate) fn register(cx: &mut App) {
  cx.bar_mut().register::<PageButton<page::ClipboardPanel>>();
  cx.control_center_mut().register::<page::ClipboardPanel>();
}
