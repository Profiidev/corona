//! The audio page, its bar button, the privacy widget, the volume sliders and OSDs

use corona_surface::{bar::BarExt, control_center::ControlCenterExt, dashboard::DashboardExt};
use gpui_kit::App;

pub mod icons;
mod osd;
pub mod page;
pub mod privacy;
mod slider;
mod widget;

pub(crate) fn register(cx: &mut App) {
  privacy::init_filter(cx);
  cx.bar_mut()
    .register::<widget::AudioButton>()
    .register::<privacy::Privacy>();
  cx.control_center_mut().register::<page::AudioPanel>();
  cx.dashboard_mut()
    .slider::<slider::Output>()
    .slider::<slider::Input>();
  osd::init(cx);
}
