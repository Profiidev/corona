pub(crate) mod dashboard;
mod displays;

use corona_surface::{
  control_center::{ControlCenter, ControlCenterExt},
  panel::AppPanelExt,
};
use gpui_kit::App;

pub use crate::control_center::{dashboard::DashboardPanel, displays::DisplaysPanel};

pub fn register_panels(cx: &mut App) {
  cx.panel().register::<ControlCenter>();
  cx.control_center_mut()
    .register::<DashboardPanel>()
    .register::<DisplaysPanel>();
  corona_surface::control_center::warn_bad_ids(cx);
  dashboard::register(cx);
  corona_surface::dashboard::warn_bad_ids(cx);
}
