//! The media page, its bar button, the player widget and the dashboard's media card

use corona_surface::{
  bar::{BarExt, PageButton},
  control_center::ControlCenterExt,
  dashboard::DashboardExt,
};
use gpui_kit::App;

mod card;
pub mod page;
pub mod player;

pub(crate) fn register(cx: &mut App) {
  cx.bar_mut()
    .register::<PageButton<page::MediaPanel>>()
    .register::<player::ActivePlayer>();
  cx.control_center_mut().register::<page::MediaPanel>();
  cx.dashboard_mut().card::<card::MediaCard>();
}
