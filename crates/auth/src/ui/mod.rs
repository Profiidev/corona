//! The lock screen, the polkit dialog and the dashboard's session menu

use corona_surface::dashboard::DashboardExt;
use gpui_kit::{App, AppContext};

pub mod lock;
mod polkit;
pub mod session;

pub(crate) fn register(cx: &mut App) {
  lock::init(cx);
  polkit::init(cx);
  cx.dashboard_mut()
    .button(|_, cx| cx.new(session::PowerMenu::new).into());
}
