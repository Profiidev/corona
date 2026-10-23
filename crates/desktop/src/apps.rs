//! Every installed application with what launching it takes

use std::{
  collections::HashSet,
  path::{Path, PathBuf},
  process::{Command, Stdio},
  sync::{Arc, RwLock},
  time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use freedesktop_desktop_entry::{DesktopEntry, Iter, default_paths, get_languages_from_env};

#[derive(Clone, Debug, PartialEq)]
pub struct App {
  /// The desktop file id, like `org.mozilla.firefox`
  pub id: String,
  pub name: String,
  pub generic_name: Option<String>,
  pub comment: Option<String>,
  pub icon: Option<String>,
  /// What `Exec=` runs, field codes dropped
  pub exec: Vec<String>,
  /// Directory to run in
  pub working_dir: Option<PathBuf>,
  pub keywords: Vec<String>,
  pub categories: Vec<String>,
  pub terminal: bool,
  /// `NoDisplay=true`: left out of searches, still in "Browse apps"
  pub no_display: bool,
  pub startup_wm_class: Option<String>,
  pub path: PathBuf,
  pub actions: Vec<AppAction>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AppAction {
  pub id: String,
  pub name: String,
  pub icon: Option<String>,
  pub exec: Vec<String>,
}

impl App {
  /// Lowercased names a window or stream of this app may report
  pub fn window_classes(&self) -> Vec<String> {
    let mut classes = vec![self.id.to_lowercase()];
    classes.extend(self.id.rsplit('.').next().map(str::to_lowercase));
    classes.extend(self.startup_wm_class.as_deref().map(str::to_lowercase));
    classes.extend(
      self
        .exec
        .first()
        .and_then(|e| e.rsplit('/').next())
        .map(str::to_lowercase),
    );
    classes.dedup();
    classes
  }

  /// The icon and class names [`crate::entry::icon_for_names`] understands, best first
  pub fn icon_names(&self) -> Vec<String> {
    let mut names: Vec<String> = self.icon.iter().cloned().collect();
    names.push(self.id.clone());
    names
  }
}

fn read(entry: &DesktopEntry, locales: &[String]) -> Option<App> {
  if entry.hidden() || entry.type_().is_some_and(|t| t != "Application") {
    return None;
  }
  let exec = entry.parse_exec().ok()?;
  if exec.is_empty() {
    return None;
  }
  let actions = entry
    .actions()
    .unwrap_or_default()
    .into_iter()
    .filter_map(|action| {
      Some(AppAction {
        id: action.to_string(),
        name: entry.action_name(action, locales)?.to_string(),
        icon: entry.action_entry(action, "Icon").map(str::to_string),
        exec: entry
          .parse_exec_action(action)
          .ok()
          .filter(|e| !e.is_empty())?,
      })
    })
    .collect();
  Some(App {
    id: entry.appid.to_string(),
    name: entry.name(locales)?.to_string(),
    generic_name: entry.generic_name(locales).map(|n| n.to_string()),
    comment: entry.comment(locales).map(|c| c.to_string()),
    icon: entry.icon().map(str::to_string),
    exec,
    working_dir: entry.path().map(PathBuf::from),
    keywords: entry
      .keywords(locales)
      .unwrap_or_default()
      .into_iter()
      .filter(|k| !k.is_empty())
      .map(|k| k.to_string())
      .collect(),
    categories: entry
      .categories()
      .unwrap_or_default()
      .into_iter()
      .filter(|c| !c.is_empty())
      .map(str::to_string)
      .collect(),
    terminal: entry.terminal(),
    no_display: entry.no_display(),
    startup_wm_class: entry.startup_wm_class().map(str::to_string),
    path: entry.path.clone(),
    actions,
  })
}

/// Scans the application directories; the first entry of an id wins, as the
/// directories are in priority order
pub fn scan() -> Vec<App> {
  let locales = get_languages_from_env();
  let mut seen = HashSet::new();
  let mut apps: Vec<App> = Iter::new(default_paths())
    .entries(Some(&locales))
    .filter_map(|entry| read(&entry, &locales))
    .filter(|app| seen.insert(app.id.clone()))
    .collect();
  apps.sort_by_key(|a| a.name.to_lowercase());
  apps
}

static APPS: RwLock<Option<Arc<Vec<App>>>> = RwLock::new(None);

/// The installed apps, scanned on first use
pub fn apps() -> Arc<Vec<App>> {
  if let Some(apps) = APPS.read().ok().and_then(|a| a.clone()) {
    return apps;
  }
  refresh()
}

/// Scans again, for new or removed entries
pub fn refresh() -> Arc<Vec<App>> {
  let apps = Arc::new(scan());
  if let Ok(mut cached) = APPS.write() {
    *cached = Some(apps.clone());
  }
  apps
}

/// Where desktop entries live, to watch for changes
pub fn directories() -> Vec<PathBuf> {
  default_paths().filter(|p| p.is_dir()).collect()
}

/// A systemd unit name part: anything outside `[A-Za-z0-9:_.-]` becomes `_`
fn unit_part(id: &str) -> String {
  id.chars()
    .map(
      |c| match c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '.' | '-') {
        true => c,
        false => '_',
      },
    )
    .collect()
}

/// Runs `argv` in its own transient scope, `app-corona-<id>-<n>.scope`, so it keeps
/// running when the shell restarts and has its own cgroup
pub fn spawn(id: &str, argv: &[String], dir: Option<&Path>) -> Result<()> {
  if argv.is_empty() {
    bail!("nothing to run");
  }
  let n = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_or(0, |d| d.as_nanos());
  let mut command = Command::new("systemd-run");
  command
    .args([
      "--user",
      "--scope",
      "--quiet",
      "--collect",
      &format!("--unit=app-corona-{}-{n}", unit_part(id)),
      "--",
    ])
    .args(argv)
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null());
  let home = dirs::home_dir();
  match dir.or(home.as_deref()) {
    Some(dir) if dir.is_dir() => {
      command.current_dir(dir);
    }
    _ => {}
  }
  let mut child = command.spawn().context("failed to run systemd-run")?;
  // reap it, systemd-run stays the app's process in the scope
  std::thread::spawn(move || {
    let _ = child.wait();
  });
  Ok(())
}

/// The command that runs a program in a terminal: the configured one, else
/// `xdg-terminal-exec`, else `$TERMINAL -e`
pub fn terminal(configured: &str) -> Vec<String> {
  let configured: Vec<String> = configured.split_whitespace().map(String::from).collect();
  if !configured.is_empty() {
    return configured;
  }
  if on_path("xdg-terminal-exec") {
    return vec!["xdg-terminal-exec".into()];
  }
  let terminal = std::env::var("TERMINAL").unwrap_or_else(|_| "xterm".into());
  vec![terminal, "-e".into()]
}

pub fn on_path(binary: &str) -> bool {
  std::env::var_os("PATH")
    .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(binary).is_file()))
}

/// `argv` as the app wants it run: in a terminal when its entry says so
pub fn command_for(app: &App, argv: &[String], terminal_command: &str) -> Vec<String> {
  match app.terminal {
    true => terminal(terminal_command)
      .into_iter()
      .chain(argv.iter().cloned())
      .collect(),
    false => argv.to_vec(),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn entry(text: &str) -> DesktopEntry {
    DesktopEntry::from_str(
      PathBuf::from("/x/org.example.App.desktop"),
      text,
      None::<&[&str]>,
    )
    .unwrap()
  }

  #[test]
  fn reads_an_entry() {
    let e = entry(
      "[Desktop Entry]\nType=Application\nName=Example\nGenericName=Browser\nExec=example --new %U\nIcon=example\nKeywords=web;internet;\nStartupWMClass=ExampleWM\nActions=private;\n\n[Desktop Action private]\nName=New private window\nExec=example --private\n",
    );
    let app = read(&e, &[]).unwrap();
    assert_eq!(app.id, "org.example.App");
    assert_eq!(app.exec, ["example", "--new"]);
    assert_eq!(app.keywords, ["web", "internet"]);
    assert_eq!(app.actions[0].exec, ["example", "--private"]);
    assert_eq!(app.actions[0].name, "New private window");
    assert_eq!(
      app.window_classes(),
      ["org.example.app", "app", "examplewm", "example"]
    );
  }

  #[test]
  fn skips_hidden_and_links() {
    assert!(
      read(
        &entry("[Desktop Entry]\nType=Application\nName=X\nExec=x\nHidden=true\n"),
        &[]
      )
      .is_none()
    );
    assert!(
      read(
        &entry("[Desktop Entry]\nType=Link\nName=X\nURL=https://x\n"),
        &[]
      )
      .is_none()
    );
    assert!(read(&entry("[Desktop Entry]\nType=Application\nName=X\n"), &[]).is_none());
  }

  #[test]
  fn unit_names() {
    assert_eq!(unit_part("org.mozilla.firefox"), "org.mozilla.firefox");
    assert_eq!(unit_part("my app/1"), "my_app_1");
  }

  #[test]
  fn terminal_from_config() {
    assert_eq!(terminal("kitty -e"), ["kitty", "-e"]);
  }
}
