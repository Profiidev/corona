//! What a source offers. A git source lists its plugins in `catalog.toml` at
//! the repository root, so browsing needs no plugin files; a directory source
//! is read from the manifests in it.

use std::{fs, path::Path};

use anyhow::{Result, bail};
use corona_config::plugins::is_flat_name;
use serde::Deserialize;

use crate::{
  PLUGIN_MANIFEST_FILENAME,
  plugin::{git, manifest::ManifestFile},
};

pub const CATALOG_FILENAME: &str = "catalog.toml";

/// One plugin a source offers
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogEntry {
  pub id: String,
  pub name: String,
  pub version: String,
  #[serde(default)]
  pub description: Option<String>,
  #[serde(default)]
  pub author: Option<String>,
  /// An image in the plugin's directory
  #[serde(default)]
  pub icon: Option<String>,
  /// A Markdown file in the plugin's directory
  #[serde(default)]
  pub readme: Option<String>,
  #[serde(default)]
  pub tags: Vec<String>,
  /// The plugin's directory at the repository root; the id when unset
  #[serde(default)]
  pub path: Option<String>,
}

impl CatalogEntry {
  pub fn subdir(&self) -> &str {
    self.path.as_deref().unwrap_or(&self.id)
  }

  fn validate(&self) -> Result<()> {
    if !is_flat_name(&self.id) {
      bail!("invalid id `{}`", self.id);
    }
    if !is_flat_name(self.subdir()) {
      bail!("invalid path `{}`", self.subdir());
    }
    if self.version.is_empty() {
      bail!("`{}` has no version", self.id);
    }
    for file in self.icon.iter().chain(&self.readme) {
      if !is_relative_file(file) {
        bail!("`{}`: invalid file `{file}`", self.id);
      }
    }
    Ok(())
  }

  /// From a manifest found in a directory source
  fn from_manifest(manifest: ManifestFile, subdir: String) -> Self {
    Self {
      id: manifest.id,
      name: manifest.name,
      version: manifest.version.unwrap_or_default(),
      description: manifest.description,
      author: None,
      icon: None,
      readme: None,
      tags: Vec::new(),
      path: Some(subdir),
    }
  }
}

/// A path below a plugin directory that cannot leave it
pub fn is_relative_file(path: &str) -> bool {
  !path.is_empty()
    && path
      .split('/')
      .all(|part| part != ".." && part != "." && !part.is_empty())
    && !path.contains('\\')
}

/// The rows of a `catalog.toml`. A bad row is skipped with a warning; the
/// others still count. Two rows of one id: the first wins.
pub fn parse_catalog(source: &str, text: &str) -> Result<Vec<CatalogEntry>> {
  #[derive(Deserialize)]
  struct File {
    #[serde(default)]
    plugin: Vec<toml::Value>,
  }
  let file: File = toml::from_str(text)?;
  let mut entries: Vec<CatalogEntry> = Vec::new();
  for row in file.plugin {
    let entry = row
      .try_into::<CatalogEntry>()
      .map_err(anyhow::Error::from)
      .and_then(|entry| entry.validate().map(|()| entry));
    match entry {
      Ok(entry) if entries.iter().any(|e| e.id == entry.id) => {
        tracing::warn!("source `{source}`: plugin `{}` is listed twice", entry.id)
      }
      Ok(entry) => entries.push(entry),
      Err(e) => tracing::warn!("source `{source}`: skipping a catalog row: {e}"),
    }
  }
  Ok(entries)
}

/// The plugins in the subdirectories of `dir`, from their manifests
pub fn scan_dir(dir: &Path) -> Vec<CatalogEntry> {
  let mut entries: Vec<CatalogEntry> = fs::read_dir(dir)
    .into_iter()
    .flatten()
    .flatten()
    .filter(|entry| entry.path().join(PLUGIN_MANIFEST_FILENAME).is_file())
    .filter_map(|entry| {
      let subdir = entry.file_name().to_str()?.to_string();
      match ManifestFile::read(&entry.path()) {
        Ok(manifest) => Some(CatalogEntry::from_manifest(manifest, subdir)),
        Err(e) => {
          tracing::warn!("{e:#}");
          None
        }
      }
    })
    .collect();
  entries.sort_by(|a, b| a.id.cmp(&b.id));
  entries
}

/// The catalog of a cloned git source and the revision it was read at: the
/// newest fetched one, so plugins published since the last update show up,
/// or the applied one when that is all there is.
pub fn read_git(
  source: &str,
  repo: &Path,
  local_only: bool,
) -> Result<(String, Vec<CatalogEntry>)> {
  let head = git::head(repo)?;
  let newest = git::remote_head(repo).unwrap_or_else(|_| head.clone());
  let read = |rev: &str| -> Result<Vec<CatalogEntry>> {
    let bytes = git::show_file(repo, rev, CATALOG_FILENAME, local_only)?;
    parse_catalog(source, &String::from_utf8(bytes)?)
  };
  match read(&newest) {
    Ok(entries) => Ok((newest, entries)),
    Err(e) if newest != head => {
      tracing::debug!("source `{source}`: catalog at {newest} unreadable: {e:#}");
      Ok((head.clone(), read(&head)?))
    }
    Err(e) => Err(e),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::plugin::git::fixture::Remote;

  const CATALOG: &str = r#"
[[plugin]]
id = "com.a"
name = "A"
version = "1.0.0"
tags = ["x"]
icon = "icon.png"

[[plugin]]
id = "com.b"
name = "B"
version = "2.0.0"
path = "b"

[[plugin]]
id = "../escape"
name = "Bad"
version = "1.0.0"

[[plugin]]
id = "com.c"
name = "C"
version = "1.0.0"
path = "../c"

[[plugin]]
id = "com.d"
name = "No version"

[[plugin]]
id = "com.e"
name = "E"
version = "1"
readme = "../../etc/passwd"

[[plugin]]
id = "com.a"
name = "Again"
version = "9"
"#;

  #[test]
  fn parses_rows_and_skips_bad_ones() {
    let entries = parse_catalog("s", CATALOG).unwrap();
    let ids: Vec<_> = entries.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["com.a", "com.b"]);
    assert_eq!(entries[0].subdir(), "com.a");
    assert_eq!(entries[0].name, "A");
    assert_eq!(entries[0].tags, ["x"]);
    assert_eq!(entries[1].subdir(), "b");
    assert!(parse_catalog("s", "plugin = 3").is_err());
    assert!(parse_catalog("s", "").unwrap().is_empty());
  }

  #[test]
  fn relative_files() {
    for good in ["icon.png", "assets/icon.png"] {
      assert!(is_relative_file(good), "{good}");
    }
    for bad in [
      "",
      "/etc/passwd",
      "../x",
      "a/../../x",
      "./x",
      "a//b",
      "a\\b",
    ] {
      assert!(!is_relative_file(bad), "{bad}");
    }
  }

  #[test]
  fn scans_a_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let write = |dir: &str, text: &str| {
      fs::create_dir_all(tmp.path().join(dir)).unwrap();
      fs::write(tmp.path().join(dir).join("plugin.toml"), text).unwrap();
    };
    write(
      "one",
      "id = \"com.one\"\nname = \"One\"\nversion = \"1\"\ndescription = \"d\"",
    );
    write("broken", "id =");
    fs::create_dir_all(tmp.path().join("empty")).unwrap();
    let entries = scan_dir(tmp.path());
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, "com.one");
    assert_eq!(entries[0].subdir(), "one");
    assert_eq!(entries[0].description.as_deref(), Some("d"));
    assert!(scan_dir(&tmp.path().join("missing")).is_empty());
  }

  #[test]
  fn reads_the_newest_fetched_catalog() {
    let remote = Remote::new();
    remote.write(
      CATALOG_FILENAME,
      "[[plugin]]\nid = \"a\"\nname = \"A\"\nversion = \"1\"\n",
    );
    let first = remote.commit();
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    git::ensure_repo(tmp.path(), &repo, &remote.url()).unwrap();
    let (rev, entries) = read_git("s", &repo, false).unwrap();
    assert_eq!(
      (rev.as_str(), entries[0].version.as_str()),
      (first.as_str(), "1")
    );

    remote.write(
      CATALOG_FILENAME,
      "[[plugin]]\nid = \"a\"\nname = \"A\"\nversion = \"2\"\n",
    );
    let second = remote.commit();
    git::fetch(&repo).unwrap();
    let (rev, entries) = read_git("s", &repo, false).unwrap();
    assert_eq!(
      (rev.as_str(), entries[0].version.as_str()),
      (second.as_str(), "2")
    );
    // still applied: the first
    assert_eq!(git::head(&repo).unwrap(), first);

    // a newest revision without a catalog falls back to the applied one
    remote.remove(CATALOG_FILENAME);
    remote.commit();
    git::fetch(&repo).unwrap();
    let (rev, entries) = read_git("s", &repo, false).unwrap();
    assert_eq!(
      (rev.as_str(), entries[0].version.as_str()),
      (first.as_str(), "1")
    );
  }
}
