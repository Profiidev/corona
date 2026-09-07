use std::{
  collections::HashMap,
  path::{Path, PathBuf},
  sync::OnceLock,
};

use freedesktop_desktop_entry::{Iter, default_paths, get_languages_from_env};

/// Generic icons to fall back on, best first. Only `application-x-executable`
/// is in the icon naming spec; the other two are what themes ship it as.
const FALLBACK_NAMES: [&str; 3] = [
  "application-x-executable",
  "applications-other",
  "application-default-icon",
];

/// Window class (lowercased) to the `Icon=` value of its desktop entry.
///
/// Built once: every lookup would otherwise re-walk each `applications/`
/// directory on disk. Entries installed while corona runs are not picked up.
fn icon_names() -> &'static HashMap<String, String> {
  static NAMES: OnceLock<HashMap<String, String>> = OnceLock::new();

  NAMES.get_or_init(|| {
    let locales = get_languages_from_env();
    let entries: Vec<_> = Iter::new(default_paths())
      .entries(Some(&locales))
      .filter_map(|entry| {
        Some((
          entry.appid.to_lowercase(),
          entry.startup_wm_class().map(str::to_lowercase),
          entry.icon()?.to_string(),
        ))
      })
      .collect();

    let mut names = HashMap::new();
    for (appid, _, icon) in &entries {
      names.insert(appid.clone(), icon.clone());
    }
    // StartupWMClass is the key the spec reserves for this lookup, so it wins
    // over an appid that collides with another app's window class.
    for (_, class, icon) in &entries {
      if let Some(class) = class {
        names.insert(class.clone(), icon.clone());
      }
    }
    names
  })
}

/// The user's icon theme, so themed icons win over the app-installed ones that
/// sit in `hicolor` at the end of the lookup chain.
fn icon_theme() -> &'static str {
  static THEME: OnceLock<String> = OnceLock::new();

  THEME.get_or_init(|| freedesktop_icons::default_theme_gtk().unwrap_or_else(|| "hicolor".into()))
}

/// `size` is a preference, not a filter: a theme that lacks it falls back to
/// its closest directory, so the file may come back at any resolution.
fn lookup(name: &str, size: u16) -> Option<PathBuf> {
  freedesktop_icons::lookup(name)
    .with_theme(icon_theme())
    .with_size(size)
    .with_cache()
    .find()
}

/// Resolve a window class (`class` or `initialClass` from the compositor) to an
/// icon file.
pub fn icon_for_class(class: &str, size: u16) -> Option<PathBuf> {
  let icon = icon_names().get(&class.to_lowercase())?;

  // `Icon=` may be an absolute path. Look its stem up in the theme first so a
  // themed replacement still wins, then fall back to the file the app shipped.
  let name = Path::new(icon)
    .file_stem()
    .and_then(|stem| stem.to_str())
    .unwrap_or(icon);

  lookup(name, size).or_else(|| icon.starts_with('/').then(|| PathBuf::from(icon)))
}

/// [`icon_for_class`], falling back to a generic application icon for classes
/// with no desktop entry. Still `None` when the theme ships no generic icon,
/// which is the case for a bare `hicolor`.
pub fn icon_for_class_or_default(class: &str, size: u16) -> Option<PathBuf> {
  icon_for_class(class, size).or_else(|| FALLBACK_NAMES.iter().find_map(|name| lookup(name, size)))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn unknown_class_has_no_entry_icon() {
    assert!(icon_for_class("corona-no-such-app", 24).is_none());
  }

  #[test]
  fn entries_without_startup_wm_class_are_kept() {
    // Most desktop files set no StartupWMClass; keying only off that would
    // leave the map nearly empty on any real system.
    println!("{} classes mapped", icon_names().len());
    assert!(icon_for_class("alacritty", 24).is_some() || icon_names().is_empty());
  }

  #[test]
  fn resolved_icons_exist_on_disk() {
    // Any entry on this machine will do; the map is empty on a bare CI box.
    let Some(class) = icon_names().keys().next() else {
      return;
    };

    if let Some(path) = icon_for_class_or_default(class, 24) {
      assert!(path.exists(), "{path:?} does not exist");
    }
  }
}
