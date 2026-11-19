//! The dashboard's screen ruler tile

use std::time::Duration;

use corona_components::animation::animation_duration;
use corona_surface::{
  dashboard::{QuickAction, Tile},
  panel::PanelState,
};
use gpui_kit::{App, SharedString, assets::IconName};
use rust_i18n::t;

use crate::overlay::ruler::{mode::RulerMode, state::RulerState};

/// shortcut: matches the panel's private open speed in corona_surface, export it if it drifts
const PANEL_CLOSE: Duration = Duration::from_millis(250);

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
        let delay = animation_duration(PANEL_CLOSE, cx);
        RulerState::start(RulerMode::default(), delay, cx);
      })),
    }
  }
}
