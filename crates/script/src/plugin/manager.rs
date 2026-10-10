//! The plugin lifecycle: which plugins run, enabling, disabling and removing
//! them, updating sources, and what the settings app shows. Git work runs in
//! the background; the config only changes once it succeeded.

use std::{
  collections::{BTreeMap, HashMap, HashSet},
  path::PathBuf,
  time::Duration,
};

use anyhow::{Result, bail};
use corona_config::{
  ConfigProvider, observe_section,
  plugins::{AutoUpdate, LOCAL_SOURCE, OFFICIAL_SOURCE, SourceConfig, SourceKind, is_flat_name},
};
use gpui_kit::{App, AppContext, Global, Task};

use crate::{
  ScriptManager,
  plugin::{
    catalog::CatalogEntry,
    materialize::matches_catalog,
    paths::Paths,
    registry::{self, Found},
    worker::{SourceCatalog, UpdateReport, Worker, ids},
  },
};

/// How often sources in the auto-update scope update
pub const AUTO_UPDATE_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

pub struct PluginManager {
  worker: Worker,
  active: HashMap<String, Found>,
  /// What each source offers, as last read
  catalogs: BTreeMap<String, SourceCatalog>,
  enabling: HashSet<String>,
  /// Sources being updated or read
  busy: HashSet<String>,
  plugin_errors: BTreeMap<String, String>,
  source_errors: BTreeMap<String, String>,
  /// Files being fetched for the settings app, and ones that do not exist
  assets_pending: HashSet<PathBuf>,
  assets_missing: HashSet<PathBuf>,
  /// Bumps whenever the plugins that run, or their widgets and panels, change
  revision: u64,
  signature: Vec<String>,
  _auto_update: Option<Task<()>>,
}

impl Global for PluginManager {}

/// A plugin as the settings app lists it
#[derive(Clone, Debug, PartialEq)]
pub struct PluginStatus {
  pub entry: CatalogEntry,
  /// The source it is enabled from: dev sources first, then the latest
  /// configured, then the local directory
  pub source: String,
  pub kind: SourceKind,
  /// Other sources offering the same id, which it wins over
  pub shadows: Vec<String>,
  pub enabled: bool,
  /// On disk, ready to run
  pub installed: bool,
  /// Running, from the source it is listed from
  pub running: bool,
  pub update_available: bool,
}

impl PluginManager {
  /// Sets the global up: scans what is on disk, exports enabled plugins that
  /// are missing, and starts the auto-update timer. Never waits on the network.
  pub fn init(paths: Paths, cx: &mut App) {
    cx.set_global(Self {
      worker: Worker::new(paths),
      active: HashMap::new(),
      catalogs: BTreeMap::new(),
      enabling: HashSet::new(),
      busy: HashSet::new(),
      plugin_errors: BTreeMap::new(),
      source_errors: BTreeMap::new(),
      assets_pending: HashSet::new(),
      assets_missing: HashSet::new(),
      revision: 0,
      signature: Vec::new(),
      _auto_update: None,
    });
    Self::rescan(cx);
    observe_section(cx, |c| &c.plugins, |_, cx| Self::rescan(cx));
    // for `${setting:<key>}` hosts, which a new revision restarts with
    // rereads every manifest on any setting change; scan once and
    // re-grant if settings change often
    observe_section(cx, |c| &c.plugin_settings, |_, cx| Self::rescan(cx));
    Self::heal(cx);

    let task = cx.spawn(async move |cx| {
      loop {
        cx.update(Self::auto_update);
        cx.background_executor().timer(AUTO_UPDATE_INTERVAL).await;
      }
    });
    cx.global_mut::<Self>()._auto_update = Some(task);
  }

  pub fn paths(&self) -> &Paths {
    &self.worker.paths
  }

  /// The plugins that run, by id
  pub fn active(&self) -> &HashMap<String, Found> {
    &self.active
  }

  pub fn revision(&self) -> u64 {
    self.revision
  }

  pub fn is_enabling(&self, id: &str) -> bool {
    self.enabling.contains(id)
  }

  pub fn is_busy(&self, source: &str) -> bool {
    self.busy.contains(source)
  }

  pub fn plugin_error(&self, id: &str) -> Option<&str> {
    self.plugin_errors.get(id).map(String::as_str)
  }

  pub fn source_error(&self, source: &str) -> Option<&str> {
    self.source_errors.get(source).map(String::as_str)
  }

  /// Reads which plugins run from disk again
  pub fn rescan(cx: &mut App) {
    let config = cx.config().plugins.clone();
    let settings = cx.config().plugin_settings.clone();
    let this = cx.global_mut::<Self>();
    this.active = registry::scan(&this.worker.paths, &config, &settings);
    let signature = signature(&this.active);
    if signature != this.signature {
      this.signature = signature;
      this.revision += 1;
    }
    let manifests = this
      .active
      .iter()
      .map(|(id, found)| (id.clone(), found.manifest.clone()))
      .collect();
    if cx.has_global::<ScriptManager>() {
      cx.global_mut::<ScriptManager>().set_plugins(manifests);
    }
    cx.refresh_windows();
  }

  /// Every plugin a source offers, once, from the source that would run it
  pub fn list(cx: &App) -> Vec<PluginStatus> {
    let this = cx.global::<Self>();
    let config = &cx.config().plugins;
    let mut order: Vec<&SourceConfig> = config
      .source
      .iter()
      .filter(|s| s.enabled && s.kind == SourceKind::Dev)
      .collect();
    order.extend(
      config
        .source
        .iter()
        .filter(|s| s.enabled && s.kind != SourceKind::Dev)
        .rev(),
    );
    let names = order
      .iter()
      .map(|s| (s.name.as_str(), s.kind))
      .chain([(LOCAL_SOURCE, SourceKind::Path)]);

    let mut rows: Vec<PluginStatus> = Vec::new();
    for (source, kind) in names {
      let Some(catalog) = this.catalogs.get(source) else {
        continue;
      };
      for entry in &catalog.entries {
        if let Some(row) = rows.iter_mut().find(|r| r.entry.id == entry.id) {
          row.shadows.push(source.to_string());
          continue;
        }
        let enabled = config.is_enabled(&entry.id);
        let installed_dir = match kind {
          SourceKind::Git => Some(this.paths().materialized(source).join(entry.subdir())),
          _ => None,
        };
        let installed = installed_dir
          .as_ref()
          .is_none_or(|dir| dir.join(crate::PLUGIN_MANIFEST_FILENAME).is_file());
        let update_available = enabled
          && installed
          && installed_dir
            .as_ref()
            .is_some_and(|dir| !matches_catalog(dir, entry));
        let running = this
          .active
          .get(&entry.id)
          .is_some_and(|found| found.origin.source == source);
        rows.push(PluginStatus {
          entry: entry.clone(),
          source: source.to_string(),
          kind,
          shadows: Vec::new(),
          enabled,
          installed,
          running,
          update_available,
        });
      }
    }
    rows.sort_by_key(|row| row.entry.name.to_lowercase());
    rows
  }

  /// Reads what every source offers, fetching git sources that were not
  /// fetched for a while, in the background
  pub fn refresh_catalogs(cx: &mut App) {
    let local = cx.global::<Self>().worker.local_catalog();
    cx.global_mut::<Self>()
      .catalogs
      .insert(LOCAL_SOURCE.to_string(), local);
    let sources: Vec<SourceConfig> = cx
      .config()
      .plugins
      .source
      .iter()
      .filter(|s| s.enabled)
      .cloned()
      .collect();
    for source in sources {
      Self::read_catalog(source, true, cx);
    }
    cx.refresh_windows();
  }

  fn read_catalog(source: SourceConfig, fetch: bool, cx: &mut App) {
    let this = cx.global_mut::<Self>();
    if !this.busy.insert(source.name.clone()) {
      return;
    }
    let worker = this.worker.clone();
    let name = source.name.clone();
    let task = cx.background_spawn(async move { worker.read_catalog(&source, fetch) });
    cx.spawn(async move |cx| {
      let result = task.await;
      cx.update(|cx| {
        let this = cx.global_mut::<Self>();
        this.busy.remove(&name);
        match result {
          Ok(catalog) => {
            this.source_errors.remove(&name);
            this.catalogs.insert(name, catalog);
          }
          Err(e) => {
            tracing::warn!("plugin source `{name}`: {e:#}");
            this.source_errors.insert(name, format!("{e:#}"));
          }
        }
        cx.refresh_windows();
      })
    })
    .detach();
  }

  /// Installs plugin `id` from the source that offers it, then enables it
  pub fn enable(id: &str, cx: &mut App) {
    let id = id.to_string();
    if !is_flat_name(&id) {
      cx.global_mut::<Self>()
        .plugin_errors
        .insert(id.clone(), format!("invalid plugin id `{id}`"));
      return;
    }
    let this = cx.global_mut::<Self>();
    if !this.enabling.insert(id.clone()) {
      return;
    }
    this.plugin_errors.remove(&id);
    let worker = this.worker.clone();
    let sources = cx.config().plugins.source.clone();
    cx.refresh_windows();

    let task = {
      let id = id.clone();
      cx.background_spawn(async move { worker.install(&sources, &id) })
    };
    cx.spawn(async move |cx| {
      let result = task.await;
      cx.update(|cx| {
        cx.global_mut::<Self>().enabling.remove(&id);
        let result = result.and_then(|_| {
          corona_config::update(cx, |c| {
            if !c.plugins.is_enabled(&id) {
              c.plugins.enabled.push(id.clone());
            }
          })
        });
        if let Err(e) = result {
          tracing::error!("enabling plugin `{id}`: {e:#}");
          cx.global_mut::<Self>()
            .plugin_errors
            .insert(id, format!("{e:#}"));
        }
        // the export may not change the config, when it was enabled already
        Self::rescan(cx);
      })
    })
    .detach();
  }

  /// Stops plugin `id`; its files, data and settings stay
  pub fn disable(id: &str, cx: &mut App) {
    let result = corona_config::update(cx, |c| c.plugins.enabled.retain(|e| e != id));
    Self::record(id, result, cx);
  }

  /// Disables plugin `id` and deletes the copies fetched from git sources.
  /// Plugins in directories, its data and its settings stay.
  pub fn remove(id: &str, cx: &mut App) {
    Self::disable(id, cx);
    let worker = cx.global::<Self>().worker.clone();
    let sources = cx.config().plugins.source.clone();
    let id = id.to_string();
    let task = {
      let id = id.clone();
      cx.background_spawn(async move { worker.uninstall(&sources, &id) })
    };
    cx.spawn(async move |cx| {
      let result = task.await;
      cx.update(|cx| {
        Self::record(&id, result, cx);
        Self::rescan(cx);
      })
    })
    .detach();
  }

  fn record(id: &str, result: Result<()>, cx: &mut App) {
    let this = cx.global_mut::<Self>();
    match result {
      Ok(()) => {
        this.plugin_errors.remove(id);
      }
      Err(e) => {
        tracing::error!("plugin `{id}`: {e:#}");
        this.plugin_errors.insert(id.to_string(), format!("{e:#}"));
      }
    }
    cx.refresh_windows();
  }

  /// Fetches git source `name` and updates its enabled plugins
  pub fn update_source(name: &str, cx: &mut App) {
    let Some(source) = cx
      .config()
      .plugins
      .source
      .iter()
      .find(|s| s.name == name)
      .cloned()
    else {
      return;
    };
    let enabled = cx.config().plugins.enabled.clone();
    Self::run_update(source, enabled, false, cx);
  }

  /// Updates every enabled git source
  pub fn update_all(cx: &mut App) {
    Self::update_scope(AutoUpdate::All, cx);
  }

  fn auto_update(cx: &mut App) {
    let scope = cx.config().plugins.auto_update;
    Self::update_scope(scope, cx);
  }

  fn update_scope(scope: AutoUpdate, cx: &mut App) {
    let config = cx.config().plugins.clone();
    for source in config.source {
      // never cloned: nothing of it is used
      if in_scope(scope, &source) && cx.global::<Self>().paths().repo(&source.name).exists() {
        Self::run_update(source, config.enabled.clone(), false, cx);
      }
    }
  }

  /// Exports enabled plugins of git sources that are missing on disk
  fn heal(cx: &mut App) {
    let config = cx.config().plugins.clone();
    if config.enabled.is_empty() {
      return;
    }
    for source in config.source {
      if source.enabled && source.kind == SourceKind::Git {
        Self::run_update(source, config.enabled.clone(), true, cx);
      }
    }
  }

  fn run_update(source: SourceConfig, enabled: Vec<String>, heal: bool, cx: &mut App) {
    if source.kind != SourceKind::Git || !source.enabled {
      return;
    }
    let this = cx.global_mut::<Self>();
    if !this.busy.insert(source.name.clone()) {
      return;
    }
    let worker = this.worker.clone();
    let name = source.name.clone();
    cx.refresh_windows();
    let task = cx.background_spawn(async move {
      if heal {
        worker.heal(&source, &enabled)
      } else {
        worker.update(&source, &enabled)
      }
    });
    cx.spawn(async move |cx| {
      let result = task.await;
      cx.update(|cx| Self::finish_update(&name, result, cx))
    })
    .detach();
  }

  fn finish_update(name: &str, result: Result<UpdateReport>, cx: &mut App) {
    let this = cx.global_mut::<Self>();
    this.busy.remove(name);
    let report = match result {
      Ok(report) => report,
      Err(e) => {
        tracing::warn!("updating plugin source `{name}`: {e:#}");
        this
          .source_errors
          .insert(name.to_string(), format!("{e:#}"));
        cx.refresh_windows();
        return;
      }
    };
    this.source_errors.remove(name);
    for id in &report.exported {
      this.plugin_errors.remove(id);
    }
    for (id, e) in &report.failed {
      tracing::warn!("updating plugin `{id}` from `{name}`: {e}");
      this.plugin_errors.insert(id.clone(), e.clone());
    }
    if report.changed {
      this.assets_missing.clear();
    }
    if let Some(catalog) = report.catalog {
      this.catalogs.insert(name.to_string(), catalog);
    }
    if report.changed || !report.exported.is_empty() {
      Self::rescan(cx);
    } else {
      cx.refresh_windows();
    }
  }

  /// Adds source `source`, or replaces the one of its name. A git source that
  /// moves elsewhere loses what was fetched from the old place first.
  pub fn add_source(source: SourceConfig, cx: &mut App) -> Result<()> {
    if !is_flat_name(&source.name) || source.name == LOCAL_SOURCE {
      bail!("invalid source name `{}`", source.name);
    }
    if source.location.trim().is_empty() {
      bail!("source `{}` needs a location", source.name);
    }
    let old = cx
      .config()
      .plugins
      .source
      .iter()
      .find(|s| s.name == source.name)
      .cloned();
    let moved = old.as_ref().is_some_and(|old| {
      old.kind == SourceKind::Git && (old.kind != source.kind || old.location != source.location)
    });
    let save = move |cx: &mut App| {
      let result = corona_config::update(cx, |c| {
        match c.plugins.source.iter_mut().find(|s| s.name == source.name) {
          Some(slot) => *slot = source.clone(),
          None => c.plugins.source.push(source.clone()),
        }
      });
      let saved = result.is_ok();
      Self::record_source(&source.name, result, cx);
      // so the settings app lists what it offers right away
      if saved && source.enabled {
        Self::read_catalog(source.clone(), false, cx);
      }
    };
    if !moved {
      save(cx);
      return Ok(());
    }
    let name = old.map(|o| o.name).unwrap_or_default();
    let this = cx.global_mut::<Self>();
    this.catalogs.remove(&name);
    let worker = this.worker.clone();
    let task = cx.background_spawn(async move { worker.delete_git_storage(&name) });
    cx.spawn(async move |cx| {
      let result = task.await;
      cx.update(|cx| match result {
        Ok(()) => save(cx),
        Err(e) => tracing::error!("deleting the old source's files: {e:#}"),
      })
    })
    .detach();
    Ok(())
  }

  /// Removes source `name`: disables the plugins it offers and deletes what
  /// was fetched from it. A directory source's files stay. The official source
  /// can be turned off but not removed.
  pub fn remove_source(name: &str, cx: &mut App) -> Result<()> {
    if name == OFFICIAL_SOURCE {
      bail!("the official source can be turned off, not removed");
    }
    let Some(source) = cx
      .config()
      .plugins
      .source
      .iter()
      .find(|s| s.name == name)
      .cloned()
    else {
      bail!("no plugin source `{name}`");
    };
    let this = cx.global_mut::<Self>();
    let offered = this
      .catalogs
      .remove(name)
      .map(|c| ids(&c.entries))
      .unwrap_or_default();
    this.source_errors.remove(name);
    let worker = this.worker.clone();
    corona_config::update(cx, |c| {
      c.plugins.source.retain(|s| s.name != name);
      c.plugins.enabled.retain(|id| !offered.contains(id));
    })?;
    if source.kind == SourceKind::Git {
      let name = name.to_string();
      cx.background_spawn(async move {
        if let Err(e) = worker.delete_git_storage(&name) {
          tracing::error!("deleting plugin source `{name}`: {e:#}");
        }
      })
      .detach();
    }
    Ok(())
  }

  pub fn set_source_enabled(name: &str, enabled: bool, cx: &mut App) {
    let result = corona_config::update(cx, |c| {
      if let Some(source) = c.plugins.source.iter_mut().find(|s| s.name == name) {
        source.enabled = enabled;
      }
    });
    Self::record_source(name, result, cx);
  }

  fn record_source(name: &str, result: Result<()>, cx: &mut App) {
    if let Err(e) = result {
      tracing::error!("plugin source `{name}`: {e:#}");
      cx.global_mut::<Self>()
        .source_errors
        .insert(name.to_string(), format!("{e:#}"));
    }
    cx.refresh_windows();
  }

  /// A file of a listed plugin, like its icon or README. Fetched from a git
  /// source in the background when it is not on disk yet; `None` until then.
  pub fn asset(status: &PluginStatus, file: &str, cx: &mut App) -> Option<PathBuf> {
    let source = source_config(&status.source, cx)?;
    let subdir = status.entry.subdir().to_string();
    let this = cx.global_mut::<Self>();
    if let Some(path) = this.worker.find_asset(&source, &subdir, file) {
      return Some(path);
    }
    let key = this.paths().cache(&source.name).join(&subdir).join(file);
    if source.kind != SourceKind::Git
      || this.assets_missing.contains(&key)
      || !this.assets_pending.insert(key.clone())
    {
      return None;
    }
    let worker = this.worker.clone();
    let file = file.to_string();
    let task = cx.background_spawn(async move { worker.fetch_asset(&source, &subdir, &file) });
    cx.spawn(async move |cx| {
      let result = task.await;
      cx.update(|cx| {
        let this = cx.global_mut::<Self>();
        this.assets_pending.remove(&key);
        if let Err(e) = result {
          tracing::debug!("plugin file {}: {e:#}", key.display());
          this.assets_missing.insert(key);
        }
        cx.refresh_windows();
      })
    })
    .detach();
    None
  }
}

/// The configured source `name`, or the local directory
fn source_config(name: &str, cx: &App) -> Option<SourceConfig> {
  if name == LOCAL_SOURCE {
    return Some(SourceConfig {
      name: LOCAL_SOURCE.to_string(),
      kind: SourceKind::Path,
      location: cx
        .global::<PluginManager>()
        .paths()
        .local
        .to_string_lossy()
        .into_owned(),
      enabled: true,
    });
  }
  cx.config()
    .plugins
    .source
    .iter()
    .find(|s| s.name == name)
    .cloned()
}

/// Whether `source` updates on its own under `scope`
pub fn in_scope(scope: AutoUpdate, source: &SourceConfig) -> bool {
  source.enabled
    && source.kind == SourceKind::Git
    && match scope {
      AutoUpdate::All => true,
      AutoUpdate::Official => source.is_official(),
      AutoUpdate::None => false,
    }
}

/// What the shell builds from the running plugins: ids, where they are and
/// their widgets, panels, services and grants; settings are read live
fn signature(active: &HashMap<String, Found>) -> Vec<String> {
  let mut signature: Vec<String> = active
    .values()
    .map(|found| {
      let m = &found.manifest;
      format!(
        "{} {} {:?} {:?} {:?} {:?} {:?} {:?} {:?}",
        m.id,
        m.dir.display(),
        m.version,
        m.widgets,
        m.panels,
        m.service,
        m.capabilities,
        m.modules,
        m.dbus
      )
    })
    .collect();
  signature.sort();
  signature
}

#[cfg(test)]
mod tests {
  use std::{fs, path::Path};

  use corona_config::Config;
  use gpui_kit::{self as gpui, TestAppContext};

  use super::*;
  use crate::plugin::git::fixture::Remote;
  use corona_config::plugins::PluginsConfig;

  fn plugins_config(cx: &App) -> PluginsConfig {
    cx.config().plugins.clone()
  }

  fn manifest(id: &str, version: &str) -> String {
    format!(
      "id = \"{id}\"\nname = \"{id}\"\nversion = \"{version}\"\n[widgets.w]\nview = \"main.js\"\n"
    )
  }

  fn row(id: &str, version: &str) -> String {
    format!(
      "[[plugin]]\nid = \"{id}\"\nname = \"{id}\"\nversion = \"{version}\"\nicon = \"icon.txt\"\n"
    )
  }

  struct Env {
    _tmp: tempfile::TempDir,
    base: PathBuf,
    remote: Remote,
  }

  /// Config and state in temp dirs, the user's config `user`, and a git
  /// remote offering `com.a`
  fn env(cx: &mut TestAppContext, user: impl FnOnce(&Remote, &Path) -> String) -> Env {
    for (key, _) in std::env::vars_os() {
      if key.to_string_lossy().starts_with("CORONA_") {
        unsafe { std::env::remove_var(key) };
      }
    }
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().to_path_buf();
    unsafe {
      std::env::set_var("XDG_CONFIG_HOME", base.join("config"));
      std::env::set_var("XDG_STATE_HOME", base.join("state"));
      std::env::set_var("XDG_DATA_HOME", base.join("data"));
    }
    let remote = Remote::new();
    remote
      .write("catalog.toml", &row("com.a", "1"))
      .write("com.a/plugin.toml", &manifest("com.a", "1"))
      .write("com.a/icon.txt", "icon");
    remote.commit();
    let dir = base.join("config/corona");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("user.toml"), user(&remote, &base)).unwrap();
    cx.update(|cx| {
      corona_config::load(cx).unwrap();
      PluginManager::init(Paths::from_config(), cx);
    });
    cx.run_until_parked();
    Env {
      _tmp: tmp,
      base,
      remote,
    }
  }

  fn git_user(remote: &Remote, _: &Path) -> String {
    format!(
      "[[plugins.source]]\nname = \"git\"\nkind = \"git\"\nlocation = \"{}\"\n",
      remote.url()
    )
  }

  fn config(cx: &mut TestAppContext) -> PluginsConfig {
    cx.update(|cx| plugins_config(cx))
  }

  fn active(cx: &mut TestAppContext) -> Vec<(String, String)> {
    cx.update(|cx| {
      let mut active: Vec<_> = cx
        .global::<PluginManager>()
        .active()
        .iter()
        .map(|(id, found)| (id.clone(), found.origin.source.clone()))
        .collect();
      active.sort();
      active
    })
  }

  fn plugin(dir: &Path, id: &str, version: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join("plugin.toml"), manifest(id, version)).unwrap();
  }

  #[gpui::test]
  fn enable_disable_remove_from_git(cx: &mut TestAppContext) {
    let env = env(cx, git_user);
    let revision = cx.update(|cx| cx.global::<PluginManager>().revision());
    cx.update(|cx| PluginManager::enable("com.a", cx));
    assert!(cx.update(|cx| cx.global::<PluginManager>().is_enabling("com.a")));
    cx.run_until_parked();
    assert!(!cx.update(|cx| cx.global::<PluginManager>().is_enabling("com.a")));
    assert_eq!(config(cx).enabled, ["com.a"]);
    assert_eq!(active(cx), [("com.a".to_string(), "git".to_string())]);
    assert!(cx.update(|cx| cx.global::<PluginManager>().revision()) > revision);
    let exported = env.base.join("state/corona/plugins/materialized/git/com.a");
    assert!(exported.join("plugin.toml").exists());

    cx.update(|cx| PluginManager::disable("com.a", cx));
    assert!(config(cx).enabled.is_empty());
    assert!(active(cx).is_empty());
    // disabling keeps the files
    assert!(exported.exists());

    cx.update(|cx| PluginManager::enable("com.a", cx));
    cx.run_until_parked();
    cx.update(|cx| PluginManager::remove("com.a", cx));
    cx.run_until_parked();
    assert!(config(cx).enabled.is_empty());
    assert!(!exported.exists());
  }

  #[gpui::test]
  fn failed_enable_leaves_the_config(cx: &mut TestAppContext) {
    let _env = env(cx, git_user);
    cx.update(|cx| PluginManager::enable("com.missing", cx));
    cx.run_until_parked();
    assert!(config(cx).enabled.is_empty());
    let error = cx.update(|cx| {
      cx.global::<PluginManager>()
        .plugin_error("com.missing")
        .map(str::to_string)
    });
    assert!(error.unwrap().contains("no plugin"));

    cx.update(|cx| PluginManager::enable("../x", cx));
    cx.run_until_parked();
    assert!(config(cx).enabled.is_empty());
  }

  #[gpui::test]
  fn enables_local_and_dev_plugins_in_place(cx: &mut TestAppContext) {
    let env = env(cx, |_, base| {
      format!(
        "[[plugins.source]]\nname = \"dev\"\nkind = \"dev\"\nlocation = \"{}\"\n",
        base.join("dev").display()
      )
    });
    plugin(&env.base.join("data/corona/plugins/mine"), "com.mine", "1");
    plugin(&env.base.join("data/corona/plugins/both"), "com.both", "1");
    plugin(&env.base.join("dev/both-work"), "com.both", "2");

    cx.update(|cx| {
      PluginManager::enable("com.mine", cx);
      PluginManager::enable("com.both", cx);
    });
    cx.run_until_parked();
    assert_eq!(
      active(cx),
      [
        ("com.both".to_string(), "dev".to_string()),
        ("com.mine".to_string(), "local".to_string())
      ]
    );
    // nothing was copied
    assert!(!env.base.join("state/corona/plugins/materialized").exists());

    // turning the dev source off falls back to the local copy
    cx.update(|cx| PluginManager::set_source_enabled("dev", false, cx));
    assert_eq!(active(cx)[0], ("com.both".to_string(), "local".to_string()));
  }

  #[gpui::test]
  fn a_changed_service_or_grant_is_a_new_revision(cx: &mut TestAppContext) {
    let env = env(cx, |_, _| String::new());
    let dir = env.base.join("data/corona/plugins/mine");
    plugin(&dir, "com.mine", "1");
    cx.update(|cx| PluginManager::enable("com.mine", cx));
    cx.run_until_parked();
    // the shell restarts services on a new revision only
    let rescan = |extra: &str| {
      let revision = cx.update(|cx| cx.global::<PluginManager>().revision());
      // the service is a top-level key, before the tables
      let manifest = format!("service = \"service.js\"\n{}", manifest("com.mine", "1"));
      fs::write(dir.join("plugin.toml"), manifest + extra).unwrap();
      cx.update(PluginManager::rescan);
      cx.update(|cx| cx.global::<PluginManager>().revision()) > revision
    };
    assert!(rescan(""));
    assert!(rescan("[capabilities]\nclipboard = { read = true }\n"));
    assert!(rescan("[capabilities]\ncorona = [\"weather\"]\n"));
    assert!(rescan("[capabilities.dbus]\nsession = [\"org.a\"]\n"));
    // settings are read live, nothing restarts for them
    let setting = "[[settings]]\nkey = \"k\"\nlabel = \"K\"\ntype = \"toggle\"\ndefault = true\n";
    let grant = "[capabilities.dbus]\nsession = [\"org.a\"]\n";
    assert!(!rescan(&format!("{setting}{grant}")));
  }

  #[gpui::test]
  fn a_changed_host_setting_is_a_new_revision(cx: &mut TestAppContext) {
    let env = env(cx, |_, _| String::new());
    let dir = env.base.join("data/corona/plugins/mine");
    let text = |key: &str| {
      format!("[[settings]]\nkey = \"{key}\"\nlabel = \"K\"\ntype = \"text\"\ndefault = \"\"\n")
    };
    let hosts = "[capabilities]\nnetwork = { hosts = [\"${setting:url}\"] }\n";
    plugin(&dir, "com.mine", "1");
    let manifest = manifest("com.mine", "1") + &text("url") + &text("other") + hosts;
    fs::write(dir.join("plugin.toml"), manifest).unwrap();
    cx.update(|cx| PluginManager::enable("com.mine", cx));
    cx.run_until_parked();
    let set = |key: &str, cx: &mut TestAppContext| {
      let revision = cx.update(|cx| cx.global::<PluginManager>().revision());
      cx.update(|cx| {
        corona_config::update(cx, |c| {
          let values = c.plugin_settings.entry("com.mine".into()).or_default();
          values.insert(key.into(), "https://ha.local:8123".into());
        })
        .unwrap()
      });
      cx.run_until_parked();
      cx.update(|cx| cx.global::<PluginManager>().revision()) > revision
    };
    assert!(set("url", cx));
    let reach = |cx: &mut TestAppContext| {
      cx.update(|cx| {
        let active = cx.global::<PluginManager>().active();
        active["com.mine"]
          .manifest
          .capabilities
          .may_reach("ha.local")
      })
    };
    assert!(reach(cx));
    // a setting no grant reads restarts nothing
    assert!(!set("other", cx));
  }

  #[gpui::test]
  fn updates_a_source(cx: &mut TestAppContext) {
    let env = env(cx, git_user);
    cx.update(|cx| PluginManager::enable("com.a", cx));
    cx.run_until_parked();
    env
      .remote
      .write("catalog.toml", &row("com.a", "2"))
      .write("com.a/plugin.toml", &manifest("com.a", "2"));
    env.remote.commit();

    cx.update(PluginManager::refresh_catalogs);
    cx.run_until_parked();
    let list = cx.update(|cx| PluginManager::list(cx));
    assert_eq!(list.len(), 1);
    assert!(list[0].update_available && list[0].running && list[0].installed);

    cx.update(|cx| PluginManager::update_source("git", cx));
    cx.run_until_parked();
    let list = cx.update(|cx| PluginManager::list(cx));
    assert!(!list[0].update_available);
    let version = cx.update(|cx| {
      cx.global::<PluginManager>().active()["com.a"]
        .manifest
        .version
        .clone()
    });
    assert_eq!(version.as_deref(), Some("2"));
  }

  #[gpui::test]
  fn heals_missing_exports_at_startup(cx: &mut TestAppContext) {
    let remote = Remote::new();
    remote
      .write("catalog.toml", &row("com.a", "1"))
      .write("com.a/plugin.toml", &manifest("com.a", "1"));
    remote.commit();
    let url = remote.url();
    let _env = env(cx, move |_, _| {
      format!(
        "[plugins]\nenabled = [\"com.a\"]\n[[plugins.source]]\nname = \"git\"\nkind = \"git\"\nlocation = \"{url}\"\n"
      )
    });
    assert_eq!(active(cx), [("com.a".to_string(), "git".to_string())]);
  }

  #[gpui::test]
  fn lists_plugins_once(cx: &mut TestAppContext) {
    let env = env(cx, |remote, base| {
      format!(
        "{}[[plugins.source]]\nname = \"dev\"\nkind = \"dev\"\nlocation = \"{}\"\n",
        git_user(remote, base),
        base.join("dev").display()
      )
    });
    plugin(&env.base.join("dev/a"), "com.a", "9");
    plugin(&env.base.join("data/corona/plugins/l"), "com.l", "1");
    cx.update(PluginManager::refresh_catalogs);
    cx.run_until_parked();
    let list = cx.update(|cx| PluginManager::list(cx));
    let rows: Vec<_> = list
      .iter()
      .map(|r| (r.entry.id.as_str(), r.source.as_str(), r.shadows.clone()))
      .collect();
    assert_eq!(
      rows,
      [
        ("com.a", "dev", vec!["git".to_string()]),
        ("com.l", "local", vec![]),
      ]
    );
    assert!(list.iter().all(|r| !r.enabled && !r.running && r.installed));
  }

  #[gpui::test]
  fn sources_are_added_and_removed(cx: &mut TestAppContext) {
    let env = env(cx, git_user);
    cx.update(|cx| PluginManager::enable("com.a", cx));
    cx.run_until_parked();
    cx.update(PluginManager::refresh_catalogs);
    cx.run_until_parked();
    let state = env.base.join("state/corona/plugins");
    assert!(state.join("sources/git/repo").exists());

    // invalid sources are refused
    let bad = SourceConfig {
      name: "../x".into(),
      kind: SourceKind::Path,
      location: "/x".into(),
      enabled: true,
    };
    assert!(cx.update(|cx| PluginManager::add_source(bad, cx)).is_err());
    let local = SourceConfig {
      name: LOCAL_SOURCE.into(),
      kind: SourceKind::Path,
      location: "/x".into(),
      enabled: true,
    };
    assert!(
      cx.update(|cx| PluginManager::add_source(local, cx))
        .is_err()
    );
    assert!(
      cx.update(|cx| PluginManager::remove_source("official", cx))
        .is_err()
    );
    assert!(
      cx.update(|cx| PluginManager::remove_source("nope", cx))
        .is_err()
    );

    // a git source moving elsewhere drops what was fetched
    let other = Remote::new();
    other.write("catalog.toml", "");
    other.commit();
    let moved = SourceConfig {
      name: "git".into(),
      kind: SourceKind::Git,
      location: other.url(),
      enabled: true,
    };
    cx.update(|cx| PluginManager::add_source(moved.clone(), cx))
      .unwrap();
    cx.run_until_parked();
    assert_eq!(config(cx).source, [moved]);
    assert!(!state.join("materialized/git").exists());
    // cloned again, from where it is now
    let origin = std::process::Command::new("git")
      .arg("-C")
      .arg(state.join("sources/git/repo"))
      .args(["remote", "get-url", "origin"])
      .output()
      .unwrap();
    assert_eq!(
      String::from_utf8(origin.stdout).unwrap().trim(),
      other.url()
    );
    assert!(
      cx.update(|cx| PluginManager::list(cx))
        .iter()
        .all(|r| r.source != "git")
    );

    // removing disables what it offered
    let path = SourceConfig {
      name: "p".into(),
      kind: SourceKind::Path,
      location: env.base.join("p").to_string_lossy().into_owned(),
      enabled: true,
    };
    plugin(&env.base.join("p/x"), "com.x", "1");
    cx.update(|cx| PluginManager::add_source(path, cx)).unwrap();
    cx.run_until_parked();
    // listed without asking the sources again
    let listed = cx.update(|cx| PluginManager::list(cx));
    assert!(
      listed
        .iter()
        .any(|r| r.entry.id == "com.x" && r.source == "p")
    );
    cx.update(|cx| PluginManager::enable("com.x", cx));
    cx.run_until_parked();
    assert!(config(cx).is_enabled("com.x"));
    cx.update(|cx| PluginManager::remove_source("p", cx))
      .unwrap();
    assert!(!config(cx).is_enabled("com.x"));
    assert!(config(cx).source.iter().all(|s| s.name != "p"));
    // a directory source's files are the user's
    assert!(env.base.join("p/x/plugin.toml").exists());
  }

  #[gpui::test]
  fn assets_are_fetched_once(cx: &mut TestAppContext) {
    let _env = env(cx, git_user);
    cx.update(PluginManager::refresh_catalogs);
    cx.run_until_parked();
    let error = cx.update(|cx| {
      cx.global::<PluginManager>()
        .source_error("git")
        .map(str::to_string)
    });
    assert_eq!(error, None);
    let row = cx.update(|cx| PluginManager::list(cx))[0].clone();
    assert_eq!(
      cx.update(|cx| PluginManager::asset(&row, "icon.txt", cx)),
      None
    );
    cx.run_until_parked();
    let path = cx
      .update(|cx| PluginManager::asset(&row, "icon.txt", cx))
      .unwrap();
    assert_eq!(fs::read_to_string(path).unwrap(), "icon");

    assert_eq!(
      cx.update(|cx| PluginManager::asset(&row, "missing.png", cx)),
      None
    );
    cx.run_until_parked();
    // remembered as missing: not fetched again
    cx.update(|cx| {
      assert_eq!(PluginManager::asset(&row, "missing.png", cx), None);
      assert!(cx.global::<PluginManager>().assets_pending.is_empty());
    });
  }

  #[test]
  fn auto_update_scope() {
    let official = SourceConfig::official();
    let fork = SourceConfig {
      location: "https://example.com/fork".into(),
      ..SourceConfig::official()
    };
    let path = SourceConfig {
      kind: SourceKind::Path,
      ..SourceConfig::official()
    };
    let off = SourceConfig {
      enabled: false,
      ..SourceConfig::official()
    };
    assert!(in_scope(AutoUpdate::Official, &official));
    assert!(!in_scope(AutoUpdate::Official, &fork));
    assert!(in_scope(AutoUpdate::All, &fork));
    assert!(!in_scope(AutoUpdate::All, &path));
    assert!(!in_scope(AutoUpdate::All, &off));
    assert!(!in_scope(AutoUpdate::None, &official));
  }

  #[gpui::test]
  fn the_config_global_is_required(cx: &mut TestAppContext) {
    cx.set_global(Config::default());
    let tmp = tempfile::tempdir().unwrap();
    cx.update(|cx| {
      PluginManager::init(
        Paths {
          state: tmp.path().join("s"),
          local: tmp.path().join("l"),
        },
        cx,
      )
    });
    // nothing enabled: no source is cloned at startup
    cx.run_until_parked();
    assert!(!tmp.path().join("s/sources").exists());
  }
}
