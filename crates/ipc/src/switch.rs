use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, ValueEnum)]
pub enum Switch {
  /// Turn it on
  On,
  /// Turn it off
  Off,
  /// Toggle it
  Toggle,
  /// Print on or off
  Status,
}

impl Switch {
  /// The state to switch to from `current`, none for [`Switch::Status`]
  pub fn target(self, current: bool) -> Option<bool> {
    match self {
      Switch::On => Some(true),
      Switch::Off => Some(false),
      Switch::Toggle => Some(!current),
      Switch::Status => None,
    }
  }
}

impl From<bool> for Switch {
  fn from(on: bool) -> Self {
    match on {
      true => Switch::On,
      false => Switch::Off,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn switch_target() {
    for current in [false, true] {
      assert_eq!(Switch::On.target(current), Some(true));
      assert_eq!(Switch::Off.target(current), Some(false));
      assert_eq!(Switch::Toggle.target(current), Some(!current));
      assert_eq!(Switch::Status.target(current), None);
    }
  }
}
