//! What the plugin manager does off the main thread: everything that runs git
//! or touches the network. Each call blocks; calls on one source wait for each
//! other, different sources run side by side.

use std::{
  collections::{HashMap, HashSet},
  fs,
  path::PathBuf,
  sync::{Arc, Mutex, PoisonError},
  time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use corona_config::plugins::{LOCAL_SOURCE, SourceConfig, SourceKind};

use crate::{
  PLUGIN_MANIFEST_FILENAME,
  plugin::{
    catalog::{self, CatalogEntry, is_relative_file},
    git,
    materialize::{matches_catalog, materialize},
    paths::{Paths, expand_user, path_is_inside, remove_tree_under},
  },
};

/// How long a fetched catalog counts as fresh for browsing
pub const BROWSE_FETCH_INTERVAL: Duration = Duration::from_secs(15 * 60);

/// One lock per source name
#[derive(Clone, Default)]
pub struct Locks(Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>);

impl Locks {
  fn get(&self, source: &str) -> Arc<Mutex<()>> {
    self
      .0
      .lock()
      .unwrap_or_else(PoisonError::into_inner)
      .entry(source.to_string())
      .or_default()
      .clone()
  }

  /// Runs `f` while holding the lock of `source`
  pub fn with<R>(&self, source: &str, f: impl FnOnce() -> R) -> R {
    let lock = self.get(source);
    let _guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
    f()
  }
}

/// What a source offers, as last read
#[derive(Clone, Debug, PartialEq)]
pub struct SourceCatalog {
  pub kind: SourceKind,
  /// The git revision the catalog was read at
  pub revision: Option<String>,
  pub entries: Vec<CatalogEntry>,
}

/// The result of updating one git source
#[derive(Debug, Default, PartialEq)]
pub struct UpdateReport {
  /// The applied revision moved
  pub changed: bool,
  /// Plugins exported again
  pub exported: Vec<String>,
  /// Plugins that could not be, with why; their old copies stay
  pub failed: Vec<(String, String)>,
  pub catalog: Option<SourceCatalog>,
}

#[derive(Clone)]
pub struct Worker {
  pub paths: Paths,
  pub locks: Locks,
  /// When each source was last fetched for browsing
  pub last_fetch: Arc<Mutex<HashMap<String, Instant>>>,
}

impl Worker {
  pub fn new(paths: Paths) -> Self {
    Self {
      paths,
      locks: Locks::default(),
      last_fetch: Arc::default(),
    }
  }

  fn ensure_repo(&self, source: &SourceConfig) -> Result<()> {
    if !git::available() {
      bail!("git is not installed");
    }
    git::ensure_repo(
      &self.paths.state,
      &self.paths.repo(&source.name),
      &source.location,
    )
  }

  /// Reads what `source` offers. A git source is cloned when it has not been,
  /// and fetched first when `fetch` is set and the last fetch is older than
  /// [`BROWSE_FETCH_INTERVAL`]; the applied revision never moves.
  pub fn read_catalog(&self, source: &SourceConfig, fetch: bool) -> Result<SourceCatalog> {
    match source.kind {
      SourceKind::Git => self.locks.with(&source.name, || {
        let repo = self.paths.repo(&source.name);
        let cloned = repo.exists();
        self.ensure_repo(source)?;
        if fetch && cloned && self.fetch_due(&source.name) {
          git::fetch(&repo)?;
        }
        let (revision, entries) = catalog::read_git(&source.name, &repo, false)?;
        Ok(SourceCatalog {
          kind: SourceKind::Git,
          revision: Some(revision),
          entries,
        })
      }),
      SourceKind::Path | SourceKind::Dev => {
        Ok(dir_catalog(source.kind, &expand_user(&source.location)))
      }
    }
  }

  /// The local directory's plugins
  pub fn local_catalog(&self) -> SourceCatalog {
    dir_catalog(SourceKind::Path, &self.paths.local)
  }

  fn fetch_due(&self, source: &str) -> bool {
    let mut last = self
      .last_fetch
      .lock()
      .unwrap_or_else(PoisonError::into_inner);
    let due = last
      .get(source)
      .is_none_or(|at| at.elapsed() >= BROWSE_FETCH_INTERVAL);
    if due {
      last.insert(source.to_string(), Instant::now());
    }
    due
  }

  /// Makes plugin `id` ready to run from the source that offers it, without
  /// enabling it: dev sources first, then the others latest first, then the
  /// local directory. Returns the source's name.
  pub fn install(&self, sources: &[SourceConfig], id: &str) -> Result<String> {
    let enabled = || sources.iter().filter(|s| s.enabled);
    let dev = enabled().filter(|s| s.kind == SourceKind::Dev);
    let others = enabled().filter(|s| s.kind != SourceKind::Dev).rev();
    let mut errors = Vec::new();
    for source in dev.chain(others) {
      match self.install_from(source, id) {
        Ok(true) => return Ok(source.name.clone()),
        Ok(false) => {}
        Err(e) => errors.push(format!("{}: {e:#}", source.name)),
      }
    }
    if self.local_catalog().entries.iter().any(|e| e.id == id) {
      return Ok(LOCAL_SOURCE.to_string());
    }
    let mut message = format!("no plugin `{id}` found in any source");
    if !errors.is_empty() {
      message = format!("{message} ({})", errors.join("; "));
    }
    Err(anyhow!(message))
  }

  /// Whether `source` offers `id`, exported when it is a git source
  fn install_from(&self, source: &SourceConfig, id: &str) -> Result<bool> {
    let catalog = self.read_catalog(source, false)?;
    let Some(entry) = catalog.entries.iter().find(|e| e.id == id) else {
      return Ok(false);
    };
    if let (SourceKind::Git, Some(revision)) = (source.kind, &catalog.revision) {
      self.locks.with(&source.name, || {
        materialize(&self.paths, &source.name, revision, entry)
      })?;
    }
    Ok(true)
  }

  /// Deletes the exported copies of `id` from every git source. Plugins in
  /// directories are the user's and stay.
  pub fn uninstall(&self, sources: &[SourceConfig], id: &str) -> Result<()> {
    for source in sources.iter().filter(|s| s.kind == SourceKind::Git) {
      self.locks.with(&source.name, || -> Result<()> {
        let root = self.paths.materialized(&source.name);
        for entry in dir_catalog(SourceKind::Git, &root).entries {
          if entry.id == id {
            remove_tree_under(&root, &root.join(entry.subdir()))?;
          }
        }
        Ok(())
      })?;
    }
    Ok(())
  }

  /// Fetches `source` and exports the enabled plugins it offers again where the
  /// revision moved or the copy differs, then applies the new revision. A
  /// plugin that fails keeps its old copy and is tried again next time.
  pub fn update(&self, source: &SourceConfig, enabled: &[String]) -> Result<UpdateReport> {
    if source.kind != SourceKind::Git {
      bail!("source `{}` is not a git source", source.name);
    }
    self.locks.with(&source.name, || {
      let repo = self.paths.repo(&source.name);
      self.ensure_repo(source)?;
      git::fetch(&repo)?;
      self
        .last_fetch
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(source.name.clone(), Instant::now());
      let new = git::remote_head(&repo)?;
      let changed = new != git::head(&repo)?;
      let entries = catalog::parse_catalog(
        &source.name,
        &String::from_utf8(git::show_file(
          &repo,
          &new,
          catalog::CATALOG_FILENAME,
          false,
        )?)?,
      )?;
      let mut report = UpdateReport {
        changed,
        ..Default::default()
      };
      self.export_enabled(source, &new, &entries, enabled, !changed, &mut report);
      if changed {
        git::set_head(&repo, &new)?;
        // cached READMEs and icons are of the old revision
        remove_tree_under(&self.paths.state, &self.paths.cache(&source.name)).ok();
      }
      report.catalog = Some(SourceCatalog {
        kind: SourceKind::Git,
        revision: Some(new),
        entries,
      });
      Ok(report)
    })
  }

  /// Exports the enabled plugins of a git source that are missing, so a wiped
  /// state directory or a restored config heals at startup. Reads the applied
  /// revision; the source is cloned when it is not.
  pub fn heal(&self, source: &SourceConfig, enabled: &[String]) -> Result<UpdateReport> {
    self.locks.with(&source.name, || {
      let repo = self.paths.repo(&source.name);
      self.ensure_repo(source)?;
      let head = git::head(&repo)?;
      let entries = catalog::parse_catalog(
        &source.name,
        &String::from_utf8(git::show_file(
          &repo,
          &head,
          catalog::CATALOG_FILENAME,
          false,
        )?)?,
      )?;
      let mut report = UpdateReport::default();
      self.export_enabled(source, &head, &entries, enabled, true, &mut report);
      Ok(report)
    })
  }

  fn export_enabled(
    &self,
    source: &SourceConfig,
    rev: &str,
    entries: &[CatalogEntry],
    enabled: &[String],
    skip_matching: bool,
    report: &mut UpdateReport,
  ) {
    let repo = self.paths.repo(&source.name);
    let root = self.paths.materialized(&source.name);
    for entry in entries.iter().filter(|e| enabled.contains(&e.id)) {
      if skip_matching && matches_catalog(&root.join(entry.subdir()), entry) {
        continue;
      }
      let manifest = format!("{}/{PLUGIN_MANIFEST_FILENAME}", entry.subdir());
      let result = if git::has_path(&repo, rev, &manifest) {
        materialize(&self.paths, &source.name, rev, entry)
      } else {
        Err(anyhow!("{manifest} is missing at {rev}"))
      };
      match result {
        Ok(()) => report.exported.push(entry.id.clone()),
        Err(e) => report.failed.push((entry.id.clone(), format!("{e:#}"))),
      }
    }
  }

  /// Deletes everything git keeps for `source`: the clone, the exported
  /// plugins and cached files
  pub fn delete_git_storage(&self, source: &str) -> Result<()> {
    self.locks.with(source, || {
      let state = &self.paths.state;
      remove_tree_under(state, &self.paths.source(source))?;
      remove_tree_under(state, &self.paths.materialized(source))?;
      remove_tree_under(state, &self.paths.cache(source))?;
      Ok(())
    })
  }

  /// A file of a plugin the settings app shows, like its icon. Taken from the
  /// installed copy or the source directory if there is one, else from the
  /// cache. Returns `None` when it has to be fetched with [`Self::fetch_asset`].
  pub fn find_asset(&self, source: &SourceConfig, subdir: &str, file: &str) -> Option<PathBuf> {
    if !is_relative_file(file) {
      return None;
    }
    let candidates = match source.kind {
      SourceKind::Git => vec![
        self
          .paths
          .materialized(&source.name)
          .join(subdir)
          .join(file),
        self.paths.cache(&source.name).join(subdir).join(file),
      ],
      SourceKind::Path | SourceKind::Dev => {
        vec![expand_user(&source.location).join(subdir).join(file)]
      }
    };
    // a link could point the settings app at any file
    candidates
      .into_iter()
      .find(|path| fs::symlink_metadata(path).is_ok_and(|m| m.is_file()))
  }

  /// Fetches a file of a plugin at the applied revision into the cache
  pub fn fetch_asset(&self, source: &SourceConfig, subdir: &str, file: &str) -> Result<PathBuf> {
    if source.kind != SourceKind::Git || !is_relative_file(file) {
      bail!("`{file}` cannot be fetched from `{}`", source.name);
    }
    self.locks.with(&source.name, || {
      let cache = self.paths.cache(&source.name);
      let target = cache.join(subdir).join(file);
      if !path_is_inside(&cache, &target) {
        bail!("`{file}` is outside the cache");
      }
      let bytes = git::show_file(
        &self.paths.repo(&source.name),
        "HEAD",
        &format!("{subdir}/{file}"),
        false,
      )?;
      fs::create_dir_all(target.parent().context("no parent")?)?;
      fs::write(&target, bytes)?;
      Ok(target)
    })
  }
}

/// A directory source's plugins, or a git source's exported ones
fn dir_catalog(kind: SourceKind, dir: &std::path::Path) -> SourceCatalog {
  SourceCatalog {
    kind,
    revision: None,
    entries: catalog::scan_dir(dir)
      .into_iter()
      .filter(|e| !e.subdir().starts_with('.'))
      .collect(),
  }
}

/// The ids of the plugins `entries` offers
pub fn ids(entries: &[CatalogEntry]) -> HashSet<String> {
  entries.iter().map(|e| e.id.clone()).collect()
}

#[cfg(test)]
mod tests {
  use std::path::Path;

  use super::*;
  use crate::plugin::git::fixture::Remote;

  fn manifest(id: &str, version: &str) -> String {
    format!("id = \"{id}\"\nname = \"N\"\nversion = \"{version}\"\n")
  }

  fn row(id: &str, version: &str, path: &str) -> String {
    format!(
      "[[plugin]]\nid = \"{id}\"\nname = \"N\"\nversion = \"{version}\"\npath = \"{path}\"\nicon = \"icon.txt\"\n"
    )
  }

  struct Setup {
    _tmp: tempfile::TempDir,
    worker: Worker,
    remote: Remote,
    base: PathBuf,
  }

  /// A git source offering `com.a` (in `a/`) and `com.b` (in `b/`)
  fn setup() -> Setup {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().to_path_buf();
    let worker = Worker::new(Paths {
      state: base.join("state"),
      local: base.join("local"),
    });
    let remote = Remote::new();
    remote
      .write(
        "catalog.toml",
        &(row("com.a", "1", "a") + &row("com.b", "1", "b")),
      )
      .write("a/plugin.toml", &manifest("com.a", "1"))
      .write("a/icon.txt", "icon a")
      .write("b/plugin.toml", &manifest("com.b", "1"));
    remote.commit();
    Setup {
      _tmp: tmp,
      worker,
      remote,
      base,
    }
  }

  fn git_source(name: &str, remote: &Remote) -> SourceConfig {
    SourceConfig {
      name: name.into(),
      kind: SourceKind::Git,
      location: remote.url(),
      enabled: true,
    }
  }

  fn dir_source(name: &str, kind: SourceKind, dir: &Path) -> SourceConfig {
    SourceConfig {
      name: name.into(),
      kind,
      location: dir.to_string_lossy().into_owned(),
      enabled: true,
    }
  }

  fn plugin(dir: &Path, id: &str, version: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join("plugin.toml"), manifest(id, version)).unwrap();
  }

  fn version(dir: &Path) -> String {
    crate::plugin::manifest::ManifestFile::read(dir)
      .unwrap()
      .version
      .unwrap()
  }

  #[test]
  fn reads_catalogs_of_every_kind() {
    let Setup {
      worker,
      remote,
      base,
      _tmp,
      ..
    } = setup();
    let catalog = worker
      .read_catalog(&git_source("s", &remote), true)
      .unwrap();
    assert_eq!(catalog.kind, SourceKind::Git);
    assert!(catalog.revision.is_some());
    assert_eq!(
      ids(&catalog.entries),
      HashSet::from(["com.a".to_string(), "com.b".to_string()])
    );

    plugin(&base.join("dir/x"), "com.x", "1");
    let catalog = worker
      .read_catalog(&dir_source("d", SourceKind::Dev, &base.join("dir")), true)
      .unwrap();
    assert_eq!(catalog.kind, SourceKind::Dev);
    assert_eq!(catalog.entries[0].id, "com.x");

    plugin(&worker.paths.local.join("l"), "com.l", "1");
    assert_eq!(worker.local_catalog().entries[0].id, "com.l");
  }

  #[test]
  fn browsing_fetches_at_most_every_interval() {
    let Setup {
      worker,
      remote,
      _tmp,
      ..
    } = setup();
    let source = git_source("s", &remote);
    // the first read clones
    worker.read_catalog(&source, true).unwrap();
    remote.write("catalog.toml", &row("com.new", "1", "new"));
    remote.commit();
    // fetched: the new catalog shows
    let catalog = worker.read_catalog(&source, true).unwrap();
    assert_eq!(catalog.entries[0].id, "com.new");

    remote.write("catalog.toml", &row("com.newer", "1", "newer"));
    remote.commit();
    // fetched moments ago, so not again
    let catalog = worker.read_catalog(&source, true).unwrap();
    assert_eq!(catalog.entries[0].id, "com.new");
    worker.last_fetch.lock().unwrap().clear();
    let catalog = worker.read_catalog(&source, true).unwrap();
    assert_eq!(catalog.entries[0].id, "com.newer");
  }

  #[test]
  fn installs_from_the_source_with_the_highest_precedence() {
    let Setup {
      worker,
      remote,
      base,
      _tmp,
      ..
    } = setup();
    let path = base.join("path");
    let dev = base.join("dev");
    plugin(&path.join("a"), "com.a", "1");
    plugin(&dev.join("only-dev"), "com.dev", "1");
    plugin(&worker.paths.local.join("loc"), "com.local", "1");
    let git = git_source("git", &remote);
    let sources = vec![
      dir_source("path", SourceKind::Path, &path),
      git.clone(),
      dir_source("dev", SourceKind::Dev, &dev),
    ];

    // `git` is later than `path`, so it offers com.a and exports it
    assert_eq!(worker.install(&sources, "com.a").unwrap(), "git");
    let exported = worker.paths.materialized("git").join("a");
    assert_eq!(version(&exported), "1");
    // only the asked-for plugin is exported
    assert!(!worker.paths.materialized("git").join("b").exists());

    assert_eq!(worker.install(&sources, "com.dev").unwrap(), "dev");
    assert_eq!(worker.install(&sources, "com.local").unwrap(), "local");
    let e = worker.install(&sources, "com.none").unwrap_err();
    assert!(e.to_string().contains("no plugin `com.none`"), "{e}");

    // a disabled source offers nothing
    let off = vec![SourceConfig {
      enabled: false,
      ..git
    }];
    assert!(worker.install(&off, "com.b").is_err());
  }

  #[test]
  fn an_unreachable_source_is_named_in_the_error() {
    let Setup { worker, _tmp, .. } = setup();
    let broken = SourceConfig {
      name: "broken".into(),
      kind: SourceKind::Git,
      location: "file:///nonexistent".into(),
      enabled: true,
    };
    let e = worker.install(&[broken], "com.a").unwrap_err();
    assert!(e.to_string().contains("broken:"), "{e}");
  }

  #[test]
  fn uninstall_deletes_exports_only() {
    let Setup {
      worker,
      remote,
      base,
      _tmp,
      ..
    } = setup();
    let path = base.join("path");
    plugin(&path.join("a"), "com.a", "1");
    let git = git_source("git", &remote);
    worker.install(std::slice::from_ref(&git), "com.a").unwrap();
    worker.install(std::slice::from_ref(&git), "com.b").unwrap();

    let sources = [git, dir_source("path", SourceKind::Path, &path)];
    worker.uninstall(&sources, "com.a").unwrap();
    assert!(!worker.paths.materialized("git").join("a").exists());
    assert!(worker.paths.materialized("git").join("b").exists());
    assert!(path.join("a/plugin.toml").exists());
  }

  #[test]
  fn update_reexports_and_applies() {
    let Setup {
      worker,
      remote,
      _tmp,
      ..
    } = setup();
    let git = git_source("git", &remote);
    let enabled = vec!["com.a".to_string(), "com.b".to_string()];
    worker.install(std::slice::from_ref(&git), "com.a").unwrap();
    worker.install(std::slice::from_ref(&git), "com.b").unwrap();
    let repo = worker.paths.repo("git");
    let old_head = git::head(&repo).unwrap();

    // nothing new: nothing to do
    let report = worker.update(&git, &enabled).unwrap();
    assert!(!report.changed && report.exported.is_empty() && report.failed.is_empty());

    // com.a gets a new version, com.b breaks
    remote
      .write(
        "catalog.toml",
        &(row("com.a", "2", "a") + &row("com.b", "2", "b")),
      )
      .write("a/plugin.toml", &manifest("com.a", "2"))
      .write("b/plugin.toml", &manifest("com.other", "2"));
    let new_head = remote.commit();
    fs::create_dir_all(worker.paths.cache("git").join("a")).unwrap();

    let report = worker.update(&git, &enabled).unwrap();
    assert!(report.changed);
    assert_eq!(report.exported, ["com.a"]);
    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].0, "com.b");
    assert_eq!(
      report.catalog.unwrap().revision.as_deref(),
      Some(new_head.as_str())
    );
    assert_ne!(old_head, new_head);
    // applied even though one plugin failed
    assert_eq!(git::head(&repo).unwrap(), new_head);
    assert_eq!(version(&worker.paths.materialized("git").join("a")), "2");
    // the broken one keeps its old copy
    assert_eq!(version(&worker.paths.materialized("git").join("b")), "1");
    // the cache was of the old revision
    assert!(!worker.paths.cache("git").exists());

    // the revision is applied, but the copy that differs is tried again
    let report = worker.update(&git, &enabled).unwrap();
    assert!(!report.changed && report.exported.is_empty());
    assert_eq!(report.failed[0].0, "com.b");

    // fixed upstream: the tip moved, so everything is exported again
    remote.write("b/plugin.toml", &manifest("com.b", "2"));
    remote.commit();
    let report = worker.update(&git, &enabled).unwrap();
    assert_eq!(report.exported, ["com.a", "com.b"]);
    assert_eq!(version(&worker.paths.materialized("git").join("b")), "2");

    let path = SourceConfig {
      kind: SourceKind::Path,
      ..git
    };
    assert!(worker.update(&path, &enabled).is_err());
  }

  #[test]
  fn heal_exports_missing_enabled_plugins() {
    let Setup {
      worker,
      remote,
      _tmp,
      ..
    } = setup();
    let git = git_source("git", &remote);
    let report = worker.heal(&git, &["com.a".to_string()]).unwrap();
    assert_eq!(report.exported, ["com.a"]);
    assert!(!worker.paths.materialized("git").join("b").exists());
    let report = worker.heal(&git, &["com.a".to_string()]).unwrap();
    assert!(report.exported.is_empty());
  }

  #[test]
  fn deletes_git_storage() {
    let Setup {
      worker,
      remote,
      _tmp,
      ..
    } = setup();
    let git = git_source("git", &remote);
    worker.install(std::slice::from_ref(&git), "com.a").unwrap();
    worker.fetch_asset(&git, "b", "icon.txt").ok();
    worker.delete_git_storage("git").unwrap();
    assert!(!worker.paths.source("git").exists());
    assert!(!worker.paths.materialized("git").exists());
    assert!(!worker.paths.cache("git").exists());
  }

  #[test]
  fn assets() {
    let Setup {
      worker,
      remote,
      base,
      _tmp,
      ..
    } = setup();
    let git = git_source("git", &remote);
    worker.read_catalog(&git, false).unwrap();
    assert_eq!(worker.find_asset(&git, "a", "icon.txt"), None);
    let fetched = worker.fetch_asset(&git, "a", "icon.txt").unwrap();
    assert_eq!(fs::read_to_string(&fetched).unwrap(), "icon a");
    assert_eq!(worker.find_asset(&git, "a", "icon.txt"), Some(fetched));
    assert!(worker.fetch_asset(&git, "a", "missing.png").is_err());
    assert!(worker.fetch_asset(&git, "a", "../../x").is_err());
    assert_eq!(worker.find_asset(&git, "a", "../../x"), None);

    // installed copies are read directly
    worker.install(std::slice::from_ref(&git), "com.a").unwrap();
    assert_eq!(
      worker.find_asset(&git, "a", "icon.txt"),
      Some(worker.paths.materialized("git").join("a/icon.txt"))
    );

    let dir = base.join("dir");
    fs::create_dir_all(dir.join("x")).unwrap();
    fs::write(dir.join("x/icon.png"), "").unwrap();
    let path = dir_source("p", SourceKind::Path, &dir);
    assert_eq!(
      worker.find_asset(&path, "x", "icon.png"),
      Some(dir.join("x/icon.png"))
    );
    assert!(worker.fetch_asset(&path, "x", "icon.png").is_err());
  }

  #[test]
  fn locks_serialize_one_source() {
    let locks = Locks::default();
    let order = Arc::new(Mutex::new(Vec::new()));
    let held = locks.get("s");
    let guard = held.lock().unwrap();
    let thread = {
      let (locks, order) = (locks.clone(), order.clone());
      std::thread::spawn(move || locks.with("s", || order.lock().unwrap().push("waiter")))
    };
    // another source is not blocked
    locks.with("other", || order.lock().unwrap().push("other"));
    std::thread::sleep(Duration::from_millis(50));
    order.lock().unwrap().push("holder");
    drop(guard);
    thread.join().unwrap();
    assert_eq!(*order.lock().unwrap(), ["other", "holder", "waiter"]);
  }
}
