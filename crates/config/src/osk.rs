use serde::{Deserialize, Serialize};

/// The on-screen keyboard
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct OskConfig {
  /// Take the seat's input method, so the keyboard sees focused text fields.
  /// Off leaves it to another input method like fcitx5
  pub enabled: bool,
  /// When a focused text field opens the keyboard
  pub auto_show: AutoShow,
  pub layout: OskLayout,
  /// Language of the suggestions and accents
  pub language: OskLanguage,
  /// Complete the word being typed
  pub suggestions: bool,
  /// Offer corrections for unknown words
  pub spell_check: bool,
  /// Rank the words you type higher, kept on this machine only
  pub learn_words: bool,
}

impl Default for OskConfig {
  fn default() -> Self {
    Self {
      enabled: true,
      auto_show: AutoShow::TabletMode,
      layout: OskLayout::Auto,
      language: OskLanguage::Auto,
      suggestions: true,
      spell_check: true,
      learn_words: true,
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutoShow {
  Never,
  Always,
  /// Only while `corona ipc osk tablet on` holds, from a Hyprland switch bind
  TabletMode,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OskLayout {
  /// Follows the compositor's active keyboard layout
  Auto,
  Qwerty,
  Qwertz,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OskLanguage {
  /// German on QWERTZ, English otherwise
  Auto,
  En,
  De,
}
