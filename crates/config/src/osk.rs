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
  /// What says the device is in tablet mode
  pub tablet_source: TabletSource,
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
      tablet_source: TabletSource::Ipc,
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
pub enum TabletSource {
  /// `corona ipc osk tablet`, from a Hyprland switch bind
  Ipc,
  /// The hinge angle sensor of a convertible: folded past 200° is tablet
  /// mode. `corona ipc osk tablet` still works until the hinge moves again
  Hinge,
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
