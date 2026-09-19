use std::{
  collections::HashMap,
  path::{Path, PathBuf},
  sync::{Mutex, OnceLock},
};

use freedesktop_desktop_entry::{DesktopEntry, Iter, default_paths, get_languages_from_env};

/// Generic icons to fall back on, best first. Only `application-x-executable`
/// is in the icon naming spec; the other two are what themes ship it as.
const FALLBACK_NAMES: [&str; 3] = [
  "application-x-executable",
  "applications-other",
  "application-default-icon",
];

/// How many keys [`keys`] produces at most, so the ranking loop covers them.
const KEY_RANKS: usize = 5;

/// Nix installs a binary as `.<name>-wrapped`, and a pipewire stream reports
/// that as its `application.process.binary`. Nothing matches it as it stands.
fn undecorate(name: &str) -> &str {
  let name = name.strip_prefix('.').unwrap_or(name);
  name.strip_suffix("-wrapped").unwrap_or(name)
}

/// The binary an `Exec=` line runs, without its path or its arguments.
fn exec_binary(exec: &str) -> Option<&str> {
  let binary = exec.split_whitespace().next()?.rsplit('/').next()?;
  (!binary.is_empty()).then(|| undecorate(binary))
}

/// Everything (lowercased) an entry is known by, weakest key first.
///
/// Anything short of this misses real applications: spotify reports the app
/// name `spotify` but the binary `.spotify-wrapped`, and Noctalia reports the
/// app name `Noctalia` against the appid `dev.noctalia.Noctalia`.
fn keys(entry: &DesktopEntry, locales: &[String]) -> Vec<String> {
  let appid = entry.appid.to_lowercase();

  let mut keys = Vec::new();
  keys.extend(entry.name(locales).map(|name| name.to_lowercase()));
  keys.extend(entry.exec().and_then(exec_binary).map(str::to_lowercase));
  // A reverse-DNS appid is named by its last segment everywhere else.
  keys.extend(appid.rsplit('.').next().map(str::to_string));
  keys.push(appid);
  keys.extend(entry.startup_wm_class().map(str::to_lowercase));
  keys
}

/// Every name an application is known by (lowercased) to the `Icon=` value of
/// its desktop entry.
///
/// Built once: every lookup would otherwise re-walk each `applications/`
/// directory on disk. Entries installed while corona runs are not picked up.
fn icon_names() -> &'static HashMap<String, String> {
  static NAMES: OnceLock<HashMap<String, String>> = OnceLock::new();

  NAMES.get_or_init(|| {
    let locales = get_languages_from_env();
    let entries: Vec<_> = Iter::new(default_paths())
      .entries(Some(&locales))
      .filter_map(|entry| Some((keys(&entry, &locales), entry.icon()?.to_string())))
      .collect();

    // Rank by key strength across all entries, not within one: an appid has to
    // beat another application's display name, not only its own. A later
    // insert wins, so the strongest key goes in last.
    let mut names = HashMap::new();
    for rank in (0..KEY_RANKS).rev() {
      for (keys, icon) in &entries {
        if let Some(key) = keys.get(rank) {
          names.insert(key.clone(), icon.clone());
        }
      }
    }
    names
  })
}

/// The user's icon theme, so themed icons win over the app-installed ones that
/// sit in `hicolor` at the end of the lookup chain.
fn icon_theme() -> &'static str {
  static THEME: OnceLock<String> = OnceLock::new();

  THEME.get_or_init(|| {
    gtk_settings_icon_theme()
      .or_else(freedesktop_icons::default_theme_gtk)
      .unwrap_or_else(|| "hicolor".into())
  })
}

fn gtk_settings_icon_theme() -> Option<String> {
  let config = dirs::config_dir()?;

  ["gtk-4.0", "gtk-3.0"].into_iter().find_map(|version| {
    let settings = std::fs::read_to_string(config.join(version).join("settings.ini")).ok()?;
    parse_icon_theme(&settings)
  })
}

fn parse_icon_theme(settings: &str) -> Option<String> {
  let value = settings
    .lines()
    .find_map(|line| line.trim().strip_prefix("gtk-icon-theme-name"))?
    .split_once('=')?
    .1;

  let value = value.trim().trim_matches('"');
  (!value.is_empty()).then(|| value.to_string())
}

/// `size` is a preference, not a filter: a theme that lacks it falls back to
/// its closest directory, so the file may come back at any resolution.
/// Memoized: a name nothing answers to costs a full walk of the theme chain
/// (~17ms here), and every render of every window icon repeats it. Misses are
/// cached too; icons installed while corona runs are not picked up.
fn lookup(name: &str, size: u16) -> Option<PathBuf> {
  static CACHE: OnceLock<Mutex<HashMap<(String, u16), Option<PathBuf>>>> = OnceLock::new();
  let cache = CACHE.get_or_init(Mutex::default);

  let key = (name.to_string(), size);
  if let Some(path) = cache.lock().ok()?.get(&key) {
    return path.clone();
  }

  let path = freedesktop_icons::lookup(name)
    .with_theme(icon_theme())
    .with_size(size)
    .with_cache()
    .find();

  cache.lock().ok()?.insert(key, path.clone());
  path
}

/// Resolve the first of `names` anything answers to. Pass what is known, best
/// first: an icon the app named itself, then its application name, then its
/// binary.
pub fn icon_for_names<'n>(names: impl IntoIterator<Item = &'n str>, size: u16) -> Option<PathBuf> {
  names.into_iter().find_map(|name| {
    let name = undecorate(name).to_lowercase();
    // A name may already be an icon (`application.icon-name`), so try the theme
    // before asking which desktop entry the name belongs to.
    lookup(&name, size).or_else(|| entry_icon(&name, size))
  })
}

fn entry_icon(name: &str, size: u16) -> Option<PathBuf> {
  let icon = icon_names().get(name)?;

  // `Icon=` may be an absolute path. Look its stem up in the theme first so a
  // themed replacement still wins, then fall back to the file the app shipped.
  let name = Path::new(icon)
    .file_stem()
    .and_then(|stem| stem.to_str())
    .unwrap_or(icon);

  lookup(name, size).or_else(|| icon.starts_with('/').then(|| PathBuf::from(icon)))
}

/// [`icon_for_names`], falling back to a generic application icon for names
/// nothing answers to. Still `None` when the theme ships no generic icon,
/// which is the case for a bare `hicolor`.
pub fn icon_for_names_or_default<'n>(
  names: impl IntoIterator<Item = &'n str>,
  size: u16,
) -> Option<PathBuf> {
  icon_for_names(names, size).or_else(|| FALLBACK_NAMES.iter().find_map(|name| lookup(name, size)))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn reads_the_icon_theme_from_gtk_settings() {
    let settings = "[Settings]\ngtk-theme-name=adw-gtk3\ngtk-icon-theme-name=kora\n";
    assert_eq!(parse_icon_theme(settings).as_deref(), Some("kora"));
    assert_eq!(
      parse_icon_theme("[Settings]\ngtk-theme-name=adw-gtk3\n"),
      None
    );
  }

  #[test]
  fn a_nix_wrapper_is_not_part_of_the_name() {
    assert_eq!(undecorate(".spotify-wrapped"), "spotify");
    assert_eq!(undecorate("spotify"), "spotify");
    assert_eq!(undecorate(".hidden"), "hidden");
  }

  #[test]
  fn an_exec_line_names_one_binary() {
    assert_eq!(exec_binary("spotify %U"), Some("spotify"));
    assert_eq!(
      exec_binary("/usr/bin/.noctalia-wrapped --daemon"),
      Some("noctalia")
    );
    assert_eq!(exec_binary(""), None);
  }

  #[test]
  fn unknown_class_has_no_entry_icon() {
    assert!(icon_for_names(["corona-no-such-app"], 24).is_none());
  }

  #[test]
  fn entries_without_startup_wm_class_are_kept() {
    // Most desktop files set no StartupWMClass; keying only off that would
    // leave the map nearly empty on any real system.
    println!("{} classes mapped", icon_names().len());
    assert!(icon_for_names(["alacritty"], 24).is_some() || icon_names().is_empty());
  }

  #[test]
  fn a_missing_icon_is_looked_up_once() {
    use std::time::Instant;

    icon_for_names_or_default(["corona-no-such-app"], 24);
    let start = Instant::now();
    icon_for_names_or_default(["corona-no-such-app"], 24);
    assert!(start.elapsed().as_millis() < 5, "lookup is not cached");
  }

  #[test]
  fn resolved_icons_exist_on_disk() {
    // Any entry on this machine will do; the map is empty on a bare CI box.
    let Some(class) = icon_names().keys().next() else {
      return;
    };

    if let Some(path) = icon_for_names_or_default([class.as_str()], 24) {
      assert!(path.exists(), "{path:?} does not exist");
    }
  }
}

