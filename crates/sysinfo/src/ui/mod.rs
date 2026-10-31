//! The system monitor page, its bar button, the resource widget and the dashboard's profile

use corona_surface::{
  bar::{BarExt, PageButton},
  control_center::ControlCenterExt,
  dashboard::DashboardExt,
};
use gpui_kit::App;

pub mod page;
mod profile;
pub mod resource;

pub(crate) fn register(cx: &mut App) {
  cx.bar_mut()
    .register::<PageButton<page::SysinfoPanel>>()
    .register::<resource::Resource>();
  cx.control_center_mut().register::<page::SysinfoPanel>();
  cx.dashboard_mut().header::<profile::ProfileCard>();
}
