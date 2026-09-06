use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default)]
pub enum AnimationSpeed {
  Slow,
  #[default]
  Normal,
  Fast,
}

impl AnimationSpeed {
  pub fn to_duration(self) -> Duration {
    match self {
      AnimationSpeed::Slow => Duration::from_millis(500),
      AnimationSpeed::Normal => Duration::from_millis(250),
      AnimationSpeed::Fast => Duration::from_millis(100),
    }
  }
}
