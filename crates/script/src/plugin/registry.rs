//! Which plugins run: the enabled ones, each from the source with the highest
//! precedence that has it. Disk only, no network.

use std::{
  collections::{BTreeMap, HashMap},
  fs,
  path::{Path, PathBuf},
};

use anyhow::Result;
use corona_config::plugins::{Approval, LOCAL_SOURCE, PluginsConfig, SourceKind};
use serde_json::{Map, Value};

use crate::{
  PLUGIN_MANIFEST_FILENAME,
  plugin::{
    manifest::{CapabilitiesFile, ManifestFile, PluginManifest},
    paths::{Paths, expand_user},
  },
};

/// Where a plugin was found
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Origin {
  pub source: String,
  pub kind: SourceKind,
}

/// A directory of plugins, one per subdirectory
#[derive(Clone, Debug, PartialEq)]
pub struct Root {
  pub origin: Origin,
  pub dir: PathBuf,
}

/// The roots from lowest to highest precedence: git and path sources in
/// config order, then the local directory, then dev sources in config order,
/// so a plugin in development wins over every installed copy
pub fn roots(paths: &Paths, config: &PluginsConfig) -> Vec<Root> {
  let sources = config.source.iter().filter(|s| s.enabled);
  let root = |source: &corona_config::plugins::SourceConfig| Root {
    origin: Origin {
      source: source.name.clone(),
      kind: source.kind,
    },
    dir: match source.kind {
      SourceKind::Git => paths.materialized(&source.name),
      SourceKind::Path | SourceKind::Dev => expand_user(&source.location),
    },
  };
  let installed = sources
    .clone()
    .filter(|s| s.kind != SourceKind::Dev)
    .map(root);
  let local = Root {
    origin: Origin {
      source: LOCAL_SOURCE.to_string(),
      kind: SourceKind::Path,
    },
    dir: paths.local.clone(),
  };
  let dev = sources.filter(|s| s.kind == SourceKind::Dev).map(root);
  installed.chain([local]).chain(dev).collect()
}

/// A plugin found on disk, enabled or not
#[derive(Clone, Debug)]
pub struct Found {
  pub origin: Origin,
  pub manifest: PluginManifest,
  /// As written, what the user approves
  pub capabilities: CapabilitiesFile,
}

/// `[plugin_settings]`, by plugin id
pub type PluginSettings = BTreeMap<String, Map<String, Value>>;

/// Every plugin in `dir`, by subdirectory, its `${setting:<key>}` hosts from
/// `settings`
pub fn scan_root(paths: &Paths, root: &Root, settings: &PluginSettings) -> Vec<Found> {
  let mut dirs: Vec<PathBuf> = fs::read_dir(&root.dir)
    .into_iter()
    .flatten()
    .flatten()
    .map(|entry| entry.path())
    // skips the manager's own temp and backup directories
    .filter(|path| !hidden(path))
    .filter(|path| path.join(PLUGIN_MANIFEST_FILENAME).is_file())
    .collect();
  dirs.sort();
  dirs
    .into_iter()
    .filter_map(|dir| match ManifestFile::read(&dir) {
      Ok(file) => {
        let id = file.id.clone();
        let data = paths.data(&id);
        Some(Found {
          origin: root.origin.clone(),
          capabilities: file.capabilities.clone(),
          manifest: PluginManifest::new(file, dir, &data, settings.get(&id)),
        })
      }
      Err(e) => {
        tracing::warn!("skipping plugin: {e:#}");
        None
      }
    })
    .collect()
}

fn hidden(path: &Path) -> bool {
  path
    .file_name()
    .is_some_and(|name| name.to_string_lossy().starts_with('.'))
}

/// The plugin each enabled id runs as: the last root that has it wins.
/// Whether it may run is [`is_approved`].
pub fn scan(
  paths: &Paths,
  config: &PluginsConfig,
  settings: &PluginSettings,
) -> HashMap<String, Found> {
  winners(paths, config, settings, |id| config.is_enabled(id))
}

/// The plugin `id` would run as once enabled
pub fn find(
  paths: &Paths,
  config: &PluginsConfig,
  settings: &PluginSettings,
  id: &str,
) -> Option<Found> {
  winners(paths, config, settings, |found| found == id).remove(id)
}

fn winners(
  paths: &Paths,
  config: &PluginsConfig,
  settings: &PluginSettings,
  wanted: impl Fn(&str) -> bool,
) -> HashMap<String, Found> {
  let mut active: HashMap<String, Found> = HashMap::new();
  for root in roots(paths, config) {
    for found in scan_root(paths, &root, settings) {
      let id = found.manifest.id.clone();
      if !wanted(&id) {
        continue;
      }
      if let Some(old) = active.get(&id) {
        if found.origin.kind == SourceKind::Dev {
          tracing::info!(
            "plugin `{id}` from dev source `{}` overrides `{}`",
            found.origin.source,
            old.origin.source
          );
        } else {
          tracing::warn!(
            "plugin `{id}` is in `{}` and `{}`, using `{}`",
            old.origin.source,
            found.origin.source,
            found.origin.source
          );
        }
      }
      active.insert(id, found);
    }
  }
  active
}

/// Whether the user approved `found` as it is: from its source, with its
/// capabilities. An update, a rescan or another source winning that changes
/// either needs a new approval.
pub fn is_approved(config: &PluginsConfig, found: &Found) -> bool {
  config.approved.get(&found.manifest.id).is_some_and(|a| {
    a.source == found.origin.source
      && toml::from_str::<CapabilitiesFile>(&a.capabilities)
        .is_ok_and(|capabilities| capabilities == found.capabilities)
  })
}

/// What approving `found` records: its source and `[capabilities]` as written
pub fn approval(found: &Found) -> Result<Approval> {
  let text = fs::read_to_string(found.manifest.dir.join(PLUGIN_MANIFEST_FILENAME))?;
  let mut file: toml::Table = toml::from_str(&text)?;
  let capabilities = match file.remove("capabilities") {
    Some(toml::Value::Table(table)) => table,
    _ => toml::Table::new(),
  };
  Ok(Approval {
    source: found.origin.source.clone(),
    capabilities: toml::to_string(&capabilities)?,
  })
}

/// What an approval's capabilities grant, for the user: `(kind, values)`,
/// the kind a table and key like `fs.read`, or a key like `corona`. Values
/// are joined with `, `, empty for a flag; off flags and empty lists are left
/// out.
pub fn permissions(capabilities: &str) -> Vec<(String, String)> {
  let table: toml::Table = toml::from_str(capabilities).unwrap_or_default();
  let mut out = Vec::new();
  for (group, value) in &table {
    match value {
      // `[[network.http]]` is a list of tables, not a table of grants
      toml::Value::Table(inner) => {
        for (key, value) in inner {
          out.extend(describe(value).map(|v| (format!("{group}.{key}"), v)));
        }
      }
      value => out.extend(describe(value).map(|v| (group.clone(), v))),
    }
  }
  out
}

fn describe(value: &toml::Value) -> Option<String> {
  use toml::Value as V;
  match value {
    V::Boolean(on) => on.then(String::new),
    V::String(s) => Some(s.clone()),
    V::Array(items) if items.is_empty() => None,
    V::Array(items) => Some(
      items
        .iter()
        .map(|item| match item {
          V::String(s) => s.clone(),
          V::Table(t) => http(t),
          other => other.to_string(),
        })
        .collect::<Vec<_>>()
        .join(", "),
    ),
    other => Some(other.to_string()),
  }
}

/// A `network.http` grant: `GET, POST https://host:port /path, /prefix…`
fn http(grant: &toml::Table) -> String {
  let get = |key: &str| grant.get(key).and_then(describe).unwrap_or_default();
  let scheme = grant
    .get("scheme")
    .and_then(|v| v.as_str())
    .unwrap_or("https");
  let port = grant
    .get("port")
    .map(|p| format!(":{p}"))
    .unwrap_or_default();
  let prefixes = get("path_prefixes").replace(", ", "…, ");
  let prefixes = if prefixes.is_empty() {
    prefixes
  } else {
    prefixes + "…"
  };
  format!(
    "{} {scheme}://{}{port} {} {prefixes}",
    get("methods"),
    get("host"),
    get("paths")
  )
  .split_whitespace()
  .collect::<Vec<_>>()
  .join(" ")
}

#[cfg(test)]
mod tests {
  use corona_config::plugins::SourceConfig;

  use super::*;

  struct Setup {
    _tmp: tempfile::TempDir,
    paths: Paths,
    base: PathBuf,
  }

  fn setup() -> Setup {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().to_path_buf();
    let paths = Paths {
      state: base.join("state"),
      local: base.join("local"),
    };
    Setup {
      _tmp: tmp,
      paths,
      base,
    }
  }

  fn plugin(dir: &Path, sub: &str, id: &str) {
    let dir = dir.join(sub);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
      dir.join(PLUGIN_MANIFEST_FILENAME),
      format!("id = \"{id}\"\nname = \"{sub}\"\n"),
    )
    .unwrap();
  }

  fn source(name: &str, kind: SourceKind, location: &Path) -> SourceConfig {
    SourceConfig {
      name: name.into(),
      kind,
      location: location.to_string_lossy().into_owned(),
      enabled: true,
    }
  }

  fn config(sources: Vec<SourceConfig>, enabled: &[&str]) -> PluginsConfig {
    PluginsConfig {
      enabled: enabled.iter().map(|s| s.to_string()).collect(),
      source: sources,
      ..Default::default()
    }
  }

  fn from(active: &HashMap<String, Found>, id: &str) -> String {
    active[id].origin.source.clone()
  }

  #[test]
  fn root_order() {
    let Setup {
      paths, base, _tmp, ..
    } = setup();
    let config = config(
      vec![
        source("dev", SourceKind::Dev, &base.join("dev")),
        source("git", SourceKind::Git, Path::new("https://x")),
        source("path", SourceKind::Path, &base.join("path")),
        SourceConfig {
          enabled: false,
          ..source("off", SourceKind::Path, &base.join("off"))
        },
      ],
      &[],
    );
    let roots: Vec<_> = roots(&paths, &config)
      .into_iter()
      .map(|r| (r.origin.source, r.dir))
      .collect();
    assert_eq!(
      roots,
      [
        ("git".to_string(), paths.materialized("git")),
        ("path".to_string(), base.join("path")),
        ("local".to_string(), paths.local.clone()),
        ("dev".to_string(), base.join("dev")),
      ]
    );
  }

  #[test]
  fn only_enabled_and_later_roots_win() {
    let Setup {
      paths, base, _tmp, ..
    } = setup();
    plugin(&paths.materialized("git"), "a", "com.a");
    plugin(&paths.materialized("git"), "b", "com.b");
    plugin(&base.join("path"), "a", "com.a");
    plugin(&paths.local, "b-local", "com.b");
    plugin(&paths.local, "c", "com.c");
    plugin(&paths.local, "off", "com.off");
    // a manager temp directory never counts
    plugin(&paths.materialized("git"), ".tmp-d-1", "com.d");

    let config = config(
      vec![
        source("git", SourceKind::Git, Path::new("https://x")),
        source("path", SourceKind::Path, &base.join("path")),
      ],
      &["com.a", "com.b", "com.c", "com.d"],
    );
    let active = scan(&paths, &config, &Default::default());
    let mut ids: Vec<_> = active.keys().cloned().collect();
    ids.sort();
    assert_eq!(ids, ["com.a", "com.b", "com.c"]);
    assert_eq!(from(&active, "com.a"), "path");
    assert_eq!(from(&active, "com.b"), "local");
    assert_eq!(active["com.b"].manifest.dir, paths.local.join("b-local"));
    assert_eq!(from(&active, "com.c"), "local");
  }

  #[test]
  fn dev_sources_override_everything() {
    let Setup {
      paths, base, _tmp, ..
    } = setup();
    let dev = base.join("dev");
    plugin(&paths.materialized("git"), "a", "com.a");
    plugin(&paths.local, "a", "com.a");
    plugin(&base.join("path"), "a", "com.a");
    plugin(&dev, "a-work", "com.a");
    plugin(&dev, "new", "com.new");

    let sources = vec![
      // listed first, still wins
      source("dev", SourceKind::Dev, &dev),
      source("git", SourceKind::Git, Path::new("https://x")),
      source("path", SourceKind::Path, &base.join("path")),
    ];
    let active = scan(
      &paths,
      &config(sources.clone(), &["com.a"]),
      &Default::default(),
    );
    assert_eq!(from(&active, "com.a"), "dev");
    assert_eq!(active["com.a"].manifest.dir, dev.join("a-work"));
    assert_eq!(active["com.a"].origin.kind, SourceKind::Dev);
    // still gated by `enabled`
    assert!(!active.contains_key("com.new"));

    // a disabled dev source overrides nothing
    let mut off = sources;
    off[0].enabled = false;
    let active = scan(&paths, &config(off, &["com.a"]), &Default::default());
    assert_eq!(from(&active, "com.a"), "local");
  }

  #[test]
  fn permissions_read_plainly() {
    let text = r#"
storage = false
corona = ["secrets"]
fs = { read = ["${pluginDir}"], write = [], execute = "*" }
clipboard = { read = true, write = false }
[[network.http]]
host = "api.x"
methods = ["GET"]
path_prefixes = ["/v1/"]
"#;
    let permissions = permissions(text);
    let kinds: Vec<_> = permissions
      .iter()
      .map(|(k, v)| format!("{k}={v}"))
      .collect();
    assert_eq!(
      kinds,
      [
        "clipboard.read=",
        "corona=secrets",
        "fs.execute=*",
        "fs.read=${pluginDir}",
        "network.http=GET https://api.x /v1/…",
      ]
    );
  }

  #[test]
  fn broken_manifests_are_skipped() {
    let Setup { paths, _tmp, .. } = setup();
    plugin(&paths.local, "good", "com.good");
    fs::create_dir_all(paths.local.join("bad")).unwrap();
    fs::write(paths.local.join("bad/plugin.toml"), "id = ").unwrap();
    let root = &roots(&paths, &config(Vec::new(), &[]))[0];
    let found = scan_root(&paths, root, &Default::default());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].manifest.id, "com.good");
    // grants are rooted in the plugin and its data directory
    assert_eq!(found[0].manifest.dir, paths.local.join("good"));
  }
}
