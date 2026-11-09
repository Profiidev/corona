//! Icons and sections by name, for what is written in a plugin manifest

use gpui_kit::assets::IconName;

use crate::command::Section;

/// The bundled icon with this kebab-case file name, like `rotate-cw`
pub fn icon_by_name(name: &str) -> Option<IconName> {
  let path = format!("icons/{name}.svg");
  IconName::ALL.iter().copied().find(|i| i.path() == path)
}

impl Section {
  /// The section a manifest names; the transient `running` one is not offered
  pub fn from_name(name: &str) -> Option<Self> {
    Some(match name {
      "favorites" => Section::Favorites,
      "recent" => Section::Recent,
      "suggestions" => Section::Suggestions,
      "applications" => Section::Applications,
      "commands" => Section::Commands,
      "results" => Section::Results,
      "fallback" => Section::Fallback,
      _ => return None,
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn names_resolve() {
    assert!(icon_by_name("folder").is_some());
    assert!(icon_by_name("no-such-icon").is_none());
    assert_eq!(Section::from_name("results"), Some(Section::Results));
    assert_eq!(Section::from_name("running"), None);
  }
}
