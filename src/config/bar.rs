use serde::{Deserialize, Serialize};

use crate::config::placement::Placement;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BarConfig {
  pub placement: Placement,
  pub height: f32,
}

impl Default for BarConfig {
  fn default() -> Self {
    Self {
      placement: Placement::Top,
      height: 30.0,
    }
  }
}
