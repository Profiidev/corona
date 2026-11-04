use std::{
  collections::HashMap,
  path::{Path, PathBuf},
  sync::{Mutex, OnceLock},
};

use crate::apps::{self, App};

/// Generic icons to fall back on, best first. Only `application-x-executable`
/// is in the icon naming spec; the other two are what themes ship it as.
const FALLBACK_NAMES: [&str; 3] = [
  "application-x-executable",
  "applications-other",
  "application-default-icon",
];

/// How many keys [`keys`] has slots for.
const KEY_RANKS: usize = 5;

/// Nix installs a binary as `.<name>-wrapped`, and a pipewire stream reports
/// that as its `application.process.binary`. Nothing matches it as it stands.
fn undecorate(name: &str) -> &str {
  let name = name.strip_prefix('.').unwrap_or(name);
  name.strip_suffix("-wrapped").unwrap_or(name)
}

/// The binary an `Exec=` program names, without its path.
fn exec_binary(program: &str) -> Option<&str> {
  let binary = program.rsplit('/').next()?;
  (!binary.is_empty()).then(|| undecorate(binary))
}

/// Everything (lowercased) an app is known by, in slots weakest first. A missing
/// key leaves its slot empty so the others keep their rank.
///
/// Anything short of this misses real applications: spotify reports the app
/// name `spotify` but the binary `.spotify-wrapped`, and Noctalia reports the
/// app name `Noctalia` against the appid `dev.noctalia.Noctalia`.
fn keys(app: &App) -> [Option<String>; KEY_RANKS] {
  let appid = app.id.to_lowercase();
  [
    Some(app.name.to_lowercase()),
    app
      .exec
      .first()
      .and_then(|program| exec_binary(program))
      .map(str::to_lowercase),
    // A reverse-DNS appid is named by its last segment everywhere else.
    appid.rsplit('.').next().map(str::to_string),
    Some(appid),
    app.startup_wm_class.as_deref().map(str::to_lowercase),
  ]
}

/// What an entry is worth looking up: its `Icon=` and its display name.
#[derive(Clone)]
struct Entry {
  icon: Option<String>,
  name: Option<String>,
}

/// Rank by key strength across all entries, not within one: an appid has to
/// beat another application's display name, not only its own. A later insert
/// wins, so the strongest key goes in last.
fn rank(entries: Vec<([Option<String>; KEY_RANKS], Entry)>) -> HashMap<String, Entry> {
  let mut names = HashMap::new();
  for rank in 0..KEY_RANKS {
    for (keys, entry) in &entries {
      if let Some(key) = &keys[rank] {
        names.insert(key.clone(), entry.clone());
      }
    }
  }
  names
}

/// Every name an application is known by (lowercased) to its desktop entry.
///
/// Built once: every lookup would otherwise re-walk each `applications/`
/// directory on disk. Entries installed while corona runs are not picked up.
fn entries() -> &'static HashMap<String, Entry> {
  static NAMES: OnceLock<HashMap<String, Entry>> = OnceLock::new();

  NAMES.get_or_init(|| {
    rank(
      apps::apps()
        .iter()
        .map(|app| {
          let entry = Entry {
            icon: app.icon.clone(),
            name: Some(app.name.clone()),
          };
          (keys(app), entry)
        })
        .collect(),
    )
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
  let value = settings.lines().find_map(|line| {
    let (key, value) = line.split_once('=')?;
    (key.trim() == "gtk-icon-theme-name").then_some(value)
  })?;

  let value = value.trim().trim_matches(['"', '\'']);
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
  fn a_program_names_one_binary() {
    assert_eq!(exec_binary("spotify"), Some("spotify"));
    assert_eq!(exec_binary("/usr/bin/.noctalia-wrapped"), Some("noctalia"));
    assert_eq!(exec_binary("/opt/My App/foo"), Some("foo"));
    assert_eq!(exec_binary(""), None);
  }

  #[test]
  fn unknown_class_has_no_entry_icon() {
    assert!(icon_for_names(["corona-no-such-app"], 24).is_none());
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

  fn app(id: &str, name: &str, exec: &[&str], class: Option<&str>) -> App {
    App {
      id: id.into(),
      name: name.into(),
      generic_name: None,
      comment: None,
      icon: None,
      exec: exec.iter().map(|e| e.to_string()).collect(),
      working_dir: None,
      keywords: Vec::new(),
      categories: Vec::new(),
      terminal: false,
      no_display: false,
      startup_wm_class: class.map(str::to_string),
      path: PathBuf::new(),
      actions: Vec::new(),
    }
  }

  #[test]
  fn a_missing_key_keeps_the_rank_of_the_others() {
    let full = keys(&app(
      "org.example.app",
      "My App",
      &["my-bin"],
      Some("MyAppClass"),
    ));
    assert_eq!(
      full,
      [
        Some("my app".into()),
        Some("my-bin".into()),
        Some("app".into()),
        Some("org.example.app".into()),
        Some("myappclass".into()),
      ]
    );

    let bare = keys(&app("org.example.app", "My App", &[], None));
    assert_eq!(bare[1], None);
    assert_eq!(bare[3].as_deref(), Some("org.example.app"));
    assert_eq!(bare[4], None);
  }

  #[test]
  fn the_strongest_key_wins() {
    let a = app("org.app.a", "App A", &["bin_a"], Some("shared"));
    let b = app("org.app.b", "shared", &["bin_b"], None);
    let entry = |name: &str| Entry {
      icon: None,
      name: Some(name.into()),
    };
    let names = rank(vec![(keys(&a), entry("App A")), (keys(&b), entry("App B"))]);

    assert_eq!(names["shared"].name.as_deref(), Some("App A"));
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
    // A key that merely starts with the name is not it
    let backup_settings =
      "[Settings]\ngtk-icon-theme-name-backup=my-other-theme\ngtk-icon-theme-name=kora\n";
    assert_eq!(parse_icon_theme(backup_settings).as_deref(), Some("kora"));

    let single_quoted = "[Settings]\ngtk-icon-theme-name='kora'\n";
    assert_eq!(parse_icon_theme(single_quoted).as_deref(), Some("kora"));

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
