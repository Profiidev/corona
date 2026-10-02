use std::str::FromStr;

use anyhow::{Result, anyhow};
use gpui_kit::assets::IconName;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
  Selection,
  Monitor,
  Window,
}

impl Mode {
  pub const ALL: [Mode; 3] = [Mode::Selection, Mode::Monitor, Mode::Window];

  pub fn label(self) -> &'static str {
    match self {
      Mode::Selection => "Selection",
      Mode::Monitor => "Monitor",
      Mode::Window => "Window",
    }
  }

  pub fn icon(self) -> IconName {
    match self {
      Mode::Selection => IconName::SquareDashed,
      Mode::Monitor => IconName::Monitor,
      Mode::Window => IconName::AppWindow,
    }
  }

  pub fn hint(self) -> &'static str {
    match self {
      Mode::Selection => "Drag an area (S)",
      Mode::Monitor => "Monitor under the cursor (M)",
      Mode::Window => "Window under the cursor (W)",
    }
  }
}

impl FromStr for Mode {
  type Err = anyhow::Error;

  fn from_str(s: &str) -> Result<Self> {
    Self::ALL
      .into_iter()
      .find(|m| m.label().eq_ignore_ascii_case(s.trim()))
      .ok_or_else(|| anyhow!("unknown mode {s:?}, expected selection|monitor|window"))
  }
}
