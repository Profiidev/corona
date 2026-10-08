use clap::ValueEnum;
use gpui_kit::assets::IconName;
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
pub enum Mode {
  Selection,
  Monitor,
  Window,
}

impl Mode {
  pub const ALL: [Mode; 3] = [Mode::Selection, Mode::Monitor, Mode::Window];

  pub fn id(self) -> &'static str {
    match self {
      Mode::Selection => "screenshot-selection",
      Mode::Monitor => "screenshot-monitor",
      Mode::Window => "screenshot-window",
    }
  }

  pub fn label(self) -> Cow<'static, str> {
    match self {
      Mode::Selection => t!("app.screenshot.selection"),
      Mode::Monitor => t!("app.screenshot.monitor"),
      Mode::Window => t!("app.screenshot.window"),
    }
  }

  pub fn icon(self) -> IconName {
    match self {
      Mode::Selection => IconName::SquareDashed,
      Mode::Monitor => IconName::Monitor,
      Mode::Window => IconName::AppWindow,
    }
  }

  pub fn hint(self) -> Cow<'static, str> {
    match self {
      Mode::Selection => t!("app.screenshot.selection_hint"),
      Mode::Monitor => t!("app.screenshot.monitor_hint"),
      Mode::Window => t!("app.screenshot.window_hint"),
    }
  }
}
