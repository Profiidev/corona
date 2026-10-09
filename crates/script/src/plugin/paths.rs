use std::{
  fs, io,
  path::{Component, Path, PathBuf},
};

use corona_config::plugins::{PluginDirs, plugin_dirs};

/// Where the plugin manager keeps what it owns. Everything below `state` except
/// `data` can be deleted and is fetched again.
#[derive(Debug, Clone, PartialEq)]
pub struct Paths {
  pub state: PathBuf,
  /// The user's own plugins, the `local` source
  pub local: PathBuf,
}

impl Paths {
  pub fn from_config() -> Self {
    let PluginDirs { local, state } = plugin_dirs();
    Self { state, local }
  }

  /// `sources/<source>`, everything git keeps for one source
  pub fn source(&self, source: &str) -> PathBuf {
    self.state.join("sources").join(source)
  }

  /// The blobless clone of a git source; never loaded from
  pub fn repo(&self, source: &str) -> PathBuf {
    self.source(source).join("repo")
  }

  /// The exported plugins of a git source, what runs
  pub fn materialized(&self, source: &str) -> PathBuf {
    self.state.join("materialized").join(source)
  }

  /// Files the settings app shows (READMEs, icons) fetched from a git source
  pub fn cache(&self, source: &str) -> PathBuf {
    self.state.join("cache").join(source)
  }

  /// A plugin's own data, which survives updates and removal
  pub fn data(&self, id: &str) -> PathBuf {
    self.state.join("data").join(id)
  }
}

/// Whether `path` is strictly below `parent`, judged on the paths alone: a
/// `..` anywhere below `parent` is not inside
pub fn path_is_inside(parent: &Path, path: &Path) -> bool {
  match path.strip_prefix(parent) {
    Ok(rest) => {
      rest.components().next().is_some()
        && rest.components().all(|c| matches!(c, Component::Normal(_)))
    }
    Err(_) => false,
  }
}

/// Removes `path` and everything in it, but only when it is inside `parent`, so
/// a bad name can never delete outside the directories the manager owns
pub fn remove_tree_under(parent: &Path, path: &Path) -> io::Result<()> {
  if !path_is_inside(parent, path) {
    return Err(io::Error::new(
      io::ErrorKind::InvalidInput,
      format!("{} is not inside {}", path.display(), parent.display()),
    ));
  }
  match fs::remove_dir_all(path) {
    Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
    result => result,
  }
}

/// `path` with a leading `~/` in the home directory
pub fn expand_user(path: &str) -> PathBuf {
  corona_config::expand_home(Path::new(path))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn layout() {
    let paths = Paths {
      state: "/s".into(),
      local: "/l".into(),
    };
    assert_eq!(
      paths.repo("official"),
      Path::new("/s/sources/official/repo")
    );
    assert_eq!(paths.materialized("a"), Path::new("/s/materialized/a"));
    assert_eq!(paths.cache("a"), Path::new("/s/cache/a"));
    assert_eq!(paths.data("com.x"), Path::new("/s/data/com.x"));
  }

  #[test]
  fn inside() {
    let parent = Path::new("/s/m");
    assert!(path_is_inside(parent, Path::new("/s/m/a")));
    assert!(path_is_inside(parent, Path::new("/s/m/a/b")));
    assert!(!path_is_inside(parent, Path::new("/s/m")));
    assert!(!path_is_inside(parent, Path::new("/s/m/../x")));
    assert!(!path_is_inside(parent, Path::new("/s/m/a/../../x")));
    assert!(!path_is_inside(parent, Path::new("/s/other")));
    assert!(!path_is_inside(parent, Path::new("/s/mm")));
  }

  #[test]
  fn removes_only_inside() {
    let tmp = tempfile::tempdir().unwrap();
    let parent = tmp.path().join("parent");
    let inner = parent.join("a");
    let outside = tmp.path().join("outside");
    fs::create_dir_all(inner.join("deep")).unwrap();
    fs::create_dir_all(&outside).unwrap();

    assert!(remove_tree_under(&parent, &outside).is_err());
    assert!(remove_tree_under(&parent, &parent.join("../outside")).is_err());
    assert!(remove_tree_under(&parent, &parent).is_err());
    assert!(outside.exists() && parent.exists());

    remove_tree_under(&parent, &inner).unwrap();
    assert!(!inner.exists());
    // already gone is fine
    remove_tree_under(&parent, &inner).unwrap();
  }
}
