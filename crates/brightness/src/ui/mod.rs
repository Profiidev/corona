//! The brightness page, its bar button, slider, OSD and night light tile

use corona_surface::{
  bar::{BarExt, PageButton},
  control_center::ControlCenterExt,
  dashboard::DashboardExt,
};
use gpui_kit::App;

mod osd;
pub mod page;
mod quick_action;
pub mod slider;

pub(crate) fn register(cx: &mut App) {
  cx.bar_mut().register::<PageButton<page::BrightnessPanel>>();
  cx.control_center_mut().register::<page::BrightnessPanel>();
  cx.dashboard_mut()
    .slider::<slider::Brightness>()
    .quick_action::<quick_action::NightlightToggle>();
  osd::init(cx);
}
