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

#[cfg(test)]
mod tests {
  use super::*;
  use std::collections::HashSet;

  #[test]
  fn all_is_complete_and_ids_unique() {
    let ids: HashSet<_> = Mode::ALL.iter().map(|m| m.id()).collect();
    assert_eq!(ids.len(), Mode::ALL.len());
    assert_eq!(Mode::ALL.len(), Mode::value_variants().len());
    for m in Mode::value_variants() {
      assert!(Mode::ALL.contains(m));
    }
  }

  #[test]
  fn labels_and_hints_translated() {
    for m in Mode::ALL {
      assert!(!m.label().is_empty() && !m.label().starts_with("app."));
      assert!(!m.hint().is_empty() && !m.hint().starts_with("app."));
      assert_ne!(m.label(), m.hint());
    }
  }
}
