pub(crate) mod dashboard;

use corona_surface::{
  control_center::{ControlCenter, ControlCenterExt},
  panel::AppPanelExt,
};
use gpui_kit::App;

pub use crate::control_center::dashboard::DashboardPanel;

pub fn register_panels(cx: &mut App) {
  cx.panel().register::<ControlCenter>();
  cx.control_center_mut().register::<DashboardPanel>();
  corona_surface::control_center::warn_bad_ids(cx);
  dashboard::register(cx);
  corona_surface::dashboard::warn_bad_ids(cx);
}
