use clap::ValueEnum;
use gpui_kit::assets::IconName;
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
pub enum RulerMode {
  Bounds,
  #[default]
  Spacing,
  Horizontal,
  Vertical,
}

impl RulerMode {
  /// In toolbar order; keys `1` to `4` pick them
  pub const ALL: [RulerMode; 4] = [
    RulerMode::Bounds,
    RulerMode::Spacing,
    RulerMode::Horizontal,
    RulerMode::Vertical,
  ];

  pub fn id(self) -> &'static str {
    match self {
      RulerMode::Bounds => "ruler-bounds",
      RulerMode::Spacing => "ruler-spacing",
      RulerMode::Horizontal => "ruler-horizontal",
      RulerMode::Vertical => "ruler-vertical",
    }
  }

  pub fn label(self) -> Cow<'static, str> {
    match self {
      RulerMode::Bounds => t!("app.ruler.bounds"),
      RulerMode::Spacing => t!("app.ruler.spacing"),
      RulerMode::Horizontal => t!("app.ruler.horizontal"),
      RulerMode::Vertical => t!("app.ruler.vertical"),
    }
  }

  pub fn icon(self) -> IconName {
    match self {
      RulerMode::Bounds => IconName::SquareDashed,
      RulerMode::Spacing => IconName::RulerDimensionLine,
      RulerMode::Horizontal => IconName::MoveHorizontal,
      RulerMode::Vertical => IconName::MoveVertical,
    }
  }

  pub fn hint(self) -> Cow<'static, str> {
    match self {
      RulerMode::Bounds => t!("app.ruler.bounds_hint"),
      RulerMode::Spacing => t!("app.ruler.spacing_hint"),
      RulerMode::Horizontal => t!("app.ruler.horizontal_hint"),
      RulerMode::Vertical => t!("app.ruler.vertical_hint"),
    }
  }

  /// The mode a number key picks
  pub fn from_key(key: &str) -> Option<Self> {
    let n: usize = key.parse().ok()?;
    Self::ALL.get(n.checked_sub(1)?).copied()
  }

  /// Which axes edge detection measures
  pub fn axes(self) -> (bool, bool) {
    match self {
      RulerMode::Bounds => (false, false),
      RulerMode::Spacing => (true, true),
      RulerMode::Horizontal => (true, false),
      RulerMode::Vertical => (false, true),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::collections::HashSet;

  #[test]
  fn all_is_complete_and_ids_unique() {
    let ids: HashSet<_> = RulerMode::ALL.iter().map(|m| m.id()).collect();
    assert_eq!(ids.len(), RulerMode::ALL.len());
    assert_eq!(RulerMode::ALL.len(), RulerMode::value_variants().len());
    for m in RulerMode::value_variants() {
      assert!(RulerMode::ALL.contains(m));
    }
  }

  #[test]
  fn labels_and_hints_translated() {
    for m in RulerMode::ALL {
      assert!(!m.label().is_empty() && !m.label().starts_with("app."));
      assert!(!m.hint().is_empty() && !m.hint().starts_with("app."));
      assert_ne!(m.label(), m.hint());
    }
  }

  #[test]
  fn number_keys_pick_modes() {
    assert_eq!(RulerMode::from_key("1"), Some(RulerMode::Bounds));
    assert_eq!(RulerMode::from_key("4"), Some(RulerMode::Vertical));
    assert_eq!(RulerMode::from_key("0"), None);
    assert_eq!(RulerMode::from_key("5"), None);
    assert_eq!(RulerMode::from_key("a"), None);
  }
}
