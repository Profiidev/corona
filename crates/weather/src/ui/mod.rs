//! The weather and calendar pages, their bar buttons, the clock and the dashboard's clock card

use corona_surface::{
  bar::{BarExt, PageButton},
  control_center::ControlCenterExt,
  dashboard::DashboardExt,
};
use gpui_kit::App;

pub mod calendar;
mod card;
pub mod clock;
pub mod page;

pub use page::place_name;

pub(crate) fn register(cx: &mut App) {
  cx.bar_mut()
    .register::<PageButton<page::WeatherPanel>>()
    .register::<PageButton<calendar::CalendarPanel>>()
    .register::<clock::Clock>();
  cx.control_center_mut()
    .register::<page::WeatherPanel>()
    .register::<calendar::CalendarPanel>();
  cx.dashboard_mut().card::<card::ClockCard>();
}
