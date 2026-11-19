//! The dashboard's screen ruler tile

use corona_components::animation::animation_duration;
use corona_surface::{
  dashboard::{QuickAction, Tile},
  panel::{PANEL_OPEN_SPEED, PanelState},
};
use gpui_kit::{App, SharedString, assets::IconName};
use rust_i18n::t;

use crate::overlay::ruler::{mode::RulerMode, state::RulerState};

pub struct RulerLauncher;

impl QuickAction for RulerLauncher {
  const ID: &'static str = "ruler";

  fn title() -> SharedString {
    t!("app.dashboard.ruler").into()
  }

  fn tile(_: &App) -> Tile {
    Tile {
      icon: IconName::Ruler,
      status: t!("app.dashboard.ruler_status").into(),
      active: false,
      toggle: Some(Box::new(|cx| {
        PanelState::close("control_center", cx).ok();
        // the frames are frozen once the control center has slid away
        let delay = animation_duration(PANEL_OPEN_SPEED, cx);
        RulerState::start(RulerMode::default(), delay, cx);
      })),
    }
  }
}
