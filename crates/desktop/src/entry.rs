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

/// What an entry is worth looking up: its `Icon=` and its display name.
#[derive(Clone)]
struct Entry {
  icon: Option<String>,
  name: Option<String>,
}

/// Every name an application is known by (lowercased) to its desktop entry.
///
/// Built once: every lookup would otherwise re-walk each `applications/`
/// directory on disk. Entries installed while corona runs are not picked up.
fn entries() -> &'static HashMap<String, Entry> {
  static NAMES: OnceLock<HashMap<String, Entry>> = OnceLock::new();

  NAMES.get_or_init(|| {
    let locales = get_languages_from_env();
    let entries: Vec<_> = Iter::new(default_paths())
      .entries(Some(&locales))
      .map(|entry| {
        let found = Entry {
          icon: entry.icon().map(str::to_string),
          name: entry.name(&locales).map(|name| name.to_string()),
        };
        (keys(&entry, &locales), found)
      })
      .collect();

    // Rank by key strength across all entries, not within one: an appid has to
    // beat another application's display name, not only its own. A later
    // insert wins, so the strongest key goes in last.
    let mut names = HashMap::new();
    for rank in (0..KEY_RANKS).rev() {
      for (keys, entry) in &entries {
        if let Some(key) = keys.get(rank) {
          names.insert(key.clone(), entry.clone());
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
      .or_else(gsettings_icon_theme)
      .unwrap_or_else(|| "hicolor".into())
  })
}

fn gsettings_icon_theme() -> Option<String> {
  let output = std::process::Command::new("gsettings")
    .args(["get", "org.gnome.desktop.interface", "icon-theme"])
    .output()
    .ok()?;
  let name = String::from_utf8(output.stdout).ok()?;
  let name = name.trim().trim_matches('\'');
  (output.status.success() && !name.is_empty()).then(|| name.to_string())
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
#[allow(clippy::type_complexity)]
static CACHE: OnceLock<Mutex<HashMap<(String, u16), Option<PathBuf>>>> = OnceLock::new();

fn lookup(name: &str, size: u16) -> Option<PathBuf> {
  let cache = CACHE.get_or_init(Mutex::default);

  let key = (name.to_string(), size);
  if let Some(path) = cache.lock().ok()?.get(&key) {
    return path.clone();
  }

  let path = cosmic_freedesktop_icons::lookup(name)
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

/// The display name of the desktop entry the first of `names` answers to.
/// Pass what is known, best first, like [`icon_for_names`]. A pipewire stream
/// reports `Brave`, its entry is called `Brave Web Browser`.
pub fn name_for_names<'n>(names: impl IntoIterator<Item = &'n str>) -> Option<String> {
  names.into_iter().find_map(|name| {
    let name = undecorate(name).to_lowercase();
    entries().get(&name)?.name.clone()
  })
}

fn entry_icon(name: &str, size: u16) -> Option<PathBuf> {
  let icon = entries().get(name)?.icon.as_ref()?;
  resolve_icon_path(icon, size)
}

fn resolve_icon_path(icon: &str, size: u16) -> Option<PathBuf> {
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
    println!("{} classes mapped", entries().len());
    assert!(icon_for_names(["alacritty"], 24).is_some() || entries().is_empty());
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
  fn an_entry_answers_to_every_name_it_is_keyed_by() {
    // Any entry on this machine will do; the map is empty on a bare CI box.
    let Some((class, entry)) = entries().iter().find(|(_, entry)| entry.name.is_some()) else {
      return;
    };

    assert_eq!(name_for_names([class.as_str()]), entry.name);
    assert_eq!(name_for_names(["corona-no-such-app"]), None);
  }

  #[test]
  fn resolved_icons_exist_on_disk() {
    // Any entry on this machine will do; the map is empty on a bare CI box.
    let Some(class) = entries().keys().next() else {
      return;
    };

    if let Some(path) = icon_for_names_or_default([class.as_str()], 24) {
      assert!(path.exists(), "{path:?} does not exist");
    }
  }

  #[test]
  fn exec_binary_quoted_paths_and_escapes() {
    // Quoted executable paths with whitespace split prematurely on whitespace
    assert_eq!(exec_binary(r#""/opt/My App/bin/foo" %U"#), Some("My"));
    // Backslash escaped paths
    assert_eq!(
      exec_binary(r#"/usr/bin/foo\sbar --arg"#),
      Some(r#"foo\sbar"#)
    );
  }

  #[test]
  fn keys_positional_index_shifting() {
    let mut full = DesktopEntry {
      appid: "org.example.app".into(),
      groups: freedesktop_desktop_entry::Groups::default(),
      path: PathBuf::new(),
      ubuntu_gettext_domain: None,
    };
    full.add_desktop_entry("Name".into(), "My App".into());
    full.add_desktop_entry("Exec".into(), "my-bin %U".into());
    full.add_desktop_entry("StartupWMClass".into(), "MyAppClass".into());
    let k_full = keys(&full, &[]);
    assert_eq!(
      k_full,
      vec!["my app", "my-bin", "app", "org.example.app", "myappclass"]
    );
    assert_eq!(k_full.len(), 5);

    // Missing Name shifts exec to index 0 and startup_wm_class to index 3
    let mut no_name = DesktopEntry {
      appid: "org.example.app".into(),
      groups: freedesktop_desktop_entry::Groups::default(),
      path: PathBuf::new(),
      ubuntu_gettext_domain: None,
    };
    no_name.add_desktop_entry("Exec".into(), "my-bin %U".into());
    no_name.add_desktop_entry("StartupWMClass".into(), "MyAppClass".into());
    let k_no_name = keys(&no_name, &[]);
    assert_eq!(
      k_no_name,
      vec!["my-bin", "app", "org.example.app", "myappclass"]
    );
    assert_eq!(k_no_name[0], "my-bin"); // shifted to rank 0!
    assert_eq!(k_no_name.get(4), None); // rank 4 missing!

    // Missing both Name and Exec shifts appid.rsplit to index 0
    let no_name_no_exec = DesktopEntry {
      appid: "org.example.app".into(),
      groups: freedesktop_desktop_entry::Groups::default(),
      path: PathBuf::new(),
      ubuntu_gettext_domain: None,
    };
    let k_minimal = keys(&no_name_no_exec, &[]);
    assert_eq!(k_minimal, vec!["app", "org.example.app"]);
    assert_eq!(k_minimal.len(), 2);
  }

  #[test]
  fn ranking_loop_overwrites_strongest_key_with_weakest() {
    // Simulate entries ranking loop from lines 81-89:
    // Entry A has StartupWMClass = "shared" at rank 4 (strongest key)
    let keys_a = vec![
      "name_a".to_string(),
      "bin_a".to_string(),
      "app_a".to_string(),
      "org.app.a".to_string(),
      "shared".to_string(), // rank 4
    ];
    let entry_a = Entry {
      icon: Some("icon_a".into()),
      name: Some("App A".into()),
    };

    // Entry B has display Name = "shared" at rank 0 (weakest key)
    let keys_b = vec![
      "shared".to_string(), // rank 0
      "bin_b".to_string(),
      "app_b".to_string(),
      "org.app.b".to_string(),
      "class_b".to_string(),
    ];
    let entry_b = Entry {
      icon: Some("icon_b".into()),
      name: Some("App B".into()),
    };

    let entries = vec![(keys_a, entry_a), (keys_b, entry_b)];

    let mut names = HashMap::new();
    for rank in (0..KEY_RANKS).rev() {
      for (keys, entry) in &entries {
        if let Some(key) = keys.get(rank) {
          names.insert(key.clone(), entry.clone());
        }
      }
    }

    // Because (0..KEY_RANKS).rev() iterates rank 4 down to 0, rank 0 is inserted LAST,
    // overwriting rank 4! So "shared" maps to App B (the weakest key) instead of App A.
    assert_eq!(names.get("shared").unwrap().name.as_deref(), Some("App B"));
  }

  #[test]
  fn hidden_and_no_display_entries_not_filtered() {
    let mut entry = DesktopEntry {
      appid: "org.example.Hidden".into(),
      groups: freedesktop_desktop_entry::Groups::default(),
      path: PathBuf::new(),
      ubuntu_gettext_domain: None,
    };
    entry.add_desktop_entry("Name".into(), "Hidden App".into());
    entry.add_desktop_entry("Hidden".into(), "true".into());
    entry.add_desktop_entry("NoDisplay".into(), "true".into());
    assert!(entry.hidden());
    assert!(entry.no_display());
    // keys() still produces keys for hidden and no_display entries
    let k = keys(&entry, &[]);
    assert_eq!(k[0], "hidden app");
  }

  #[test]
  fn absolute_icon_path_returned_without_verifying_existence() {
    let nonexistent = "/opt/apps/nonexistent_icon_12345.png";
    let resolved = resolve_icon_path(nonexistent, 24);
    assert_eq!(resolved, Some(PathBuf::from(nonexistent)));
    assert!(!resolved.unwrap().exists());
  }

  #[test]
  fn parse_icon_theme_edges() {
    // Prefix collision: gtk-icon-theme-name-backup matched before gtk-icon-theme-name
    let backup_settings =
      "[Settings]\ngtk-icon-theme-name-backup=my-other-theme\ngtk-icon-theme-name=kora\n";
    assert_eq!(
      parse_icon_theme(backup_settings).as_deref(),
      Some("my-other-theme")
    );

    // Single-quoted theme name is not stripped of single quotes
    let single_quoted = "[Settings]\ngtk-icon-theme-name='kora'\n";
    assert_eq!(parse_icon_theme(single_quoted).as_deref(), Some("'kora'"));

    // Double-quoted theme name is stripped
    let double_quoted = "[Settings]\ngtk-icon-theme-name=\"kora\"\n";
    assert_eq!(parse_icon_theme(double_quoted).as_deref(), Some("kora"));

    // Spaces around =
    let spaced = "[Settings]\ngtk-icon-theme-name = kora\n";
    assert_eq!(parse_icon_theme(spaced).as_deref(), Some("kora"));
  }

  #[test]
  fn mutex_poisoning_silences_lookup() {
    let cache = CACHE.get_or_init(Mutex::default);
    let _ = std::panic::catch_unwind(|| {
      let _guard = cache.lock().unwrap();
      panic!("poison cache");
    });
    assert!(cache.is_poisoned());
    // All lookups immediately return None when mutex is poisoned
    assert_eq!(lookup("any-app", 24), None);
  }
}
