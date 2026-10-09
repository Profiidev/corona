//! Exports one plugin of a git source to the directory it runs from. The new
//! copy is checked before it replaces the old one, and replaces it whole, so a
//! half-written plugin is never seen.

use std::{fs, path::Path};

use anyhow::{Context, Result, bail};

use crate::plugin::{
  catalog::CatalogEntry,
  git,
  manifest::ManifestFile,
  paths::{Paths, path_is_inside, remove_tree_under},
};

/// Exports `entry` at `rev` from the clone of `source` into
/// `materialized/<source>/<subdir>`
pub fn materialize(paths: &Paths, source: &str, rev: &str, entry: &CatalogEntry) -> Result<()> {
  let root = paths.materialized(source);
  fs::create_dir_all(&root)?;
  let subdir = entry.subdir();
  let tmp = root.join(format!(".tmp-{subdir}-{}", uuid::Uuid::new_v4()));
  let result = export(paths, source, rev, entry, &root, &tmp);
  remove_tree_under(&root, &tmp).ok();
  result
}

fn export(
  paths: &Paths,
  source: &str,
  rev: &str,
  entry: &CatalogEntry,
  root: &Path,
  tmp: &Path,
) -> Result<()> {
  let subdir = entry.subdir();
  git::export_subdir(&paths.repo(source), rev, subdir, tmp)?;
  let staged = tmp.join(subdir);
  let manifest = ManifestFile::read(&staged)?;
  if manifest.id != entry.id {
    bail!(
      "`{subdir}` holds plugin `{}`, the catalog says `{}`",
      manifest.id,
      entry.id
    );
  }
  replace_directory(root, &staged, &root.join(subdir))
}

/// Moves `staged` to `target`, which may exist. Both must be inside `root`. The
/// old `target` is kept aside until the new one is in place, and put back if
/// that fails.
pub fn replace_directory(root: &Path, staged: &Path, target: &Path) -> Result<()> {
  if !path_is_inside(root, staged) || !path_is_inside(root, target) {
    bail!(
      "refusing to replace {} outside {}",
      target.display(),
      root.display()
    );
  }
  let name = target
    .file_name()
    .context("target has no name")?
    .to_string_lossy();
  let backup = root.join(format!(".old-{name}-{}", uuid::Uuid::new_v4()));
  let had_old = target.exists();
  if had_old {
    fs::rename(target, &backup)?;
  }
  if let Err(e) = fs::rename(staged, target) {
    if had_old {
      fs::rename(&backup, target).ok();
    }
    return Err(e.into());
  }
  if had_old {
    remove_tree_under(root, &backup).ok();
  }
  Ok(())
}

/// Whether the copy at `dir` is the plugin and version the catalog offers
pub fn matches_catalog(dir: &Path, entry: &CatalogEntry) -> bool {
  ManifestFile::read(dir).is_ok_and(|manifest| {
    manifest.id == entry.id && manifest.version.as_deref().unwrap_or_default() == entry.version
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::plugin::git::fixture::Remote;

  fn entry(id: &str, version: &str, path: Option<&str>) -> CatalogEntry {
    CatalogEntry {
      id: id.into(),
      name: id.into(),
      version: version.into(),
      description: None,
      author: None,
      icon: None,
      readme: None,
      tags: Vec::new(),
      path: path.map(Into::into),
    }
  }

  fn manifest(id: &str, version: &str) -> String {
    format!("id = \"{id}\"\nname = \"N\"\nversion = \"{version}\"\n")
  }

  struct Setup {
    _tmp: tempfile::TempDir,
    remote: Remote,
    paths: Paths,
  }

  fn setup() -> Setup {
    let tmp = tempfile::tempdir().unwrap();
    let paths = Paths {
      state: tmp.path().join("state"),
      local: tmp.path().join("local"),
    };
    let remote = Remote::new();
    remote
      .write("a/plugin.toml", &manifest("com.a", "1"))
      .write("a/main.js", "1")
      .write("wrong/plugin.toml", &manifest("com.other", "1"));
    remote.commit();
    git::ensure_repo(&paths.state, &paths.repo("s"), &remote.url()).unwrap();
    Setup {
      _tmp: tmp,
      remote,
      paths,
    }
  }

  fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<_> = fs::read_dir(dir)
      .unwrap()
      .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
      .collect();
    names.sort();
    names
  }

  #[test]
  fn exports_and_replaces() {
    let Setup {
      paths,
      remote,
      _tmp,
      ..
    } = setup();
    let head = git::head(&paths.repo("s")).unwrap();
    let a = entry("com.a", "1", Some("a"));
    materialize(&paths, "s", &head, &a).unwrap();
    let dir = paths.materialized("s").join("a");
    assert_eq!(fs::read_to_string(dir.join("main.js")).unwrap(), "1");
    assert!(matches_catalog(&dir, &a));
    assert!(!matches_catalog(&dir, &entry("com.a", "2", Some("a"))));

    remote
      .write("a/plugin.toml", &manifest("com.a", "2"))
      .write("a/main.js", "2");
    let rev = remote.commit();
    git::fetch(&paths.repo("s")).unwrap();
    materialize(&paths, "s", &rev, &entry("com.a", "2", Some("a"))).unwrap();
    assert_eq!(fs::read_to_string(dir.join("main.js")).unwrap(), "2");
    // no temp or backup directories stay behind
    assert_eq!(entries(&paths.materialized("s")), ["a"]);
  }

  #[test]
  fn a_wrong_plugin_keeps_the_old_copy() {
    let Setup {
      paths,
      remote: _remote,
      _tmp,
    } = setup();
    let head = git::head(&paths.repo("s")).unwrap();
    materialize(&paths, "s", &head, &entry("com.a", "1", Some("a"))).unwrap();

    // the catalog claims `a` holds another plugin
    let e = materialize(&paths, "s", &head, &entry("com.x", "1", Some("a"))).unwrap_err();
    assert!(e.to_string().contains("com.a"), "{e}");
    let e = materialize(&paths, "s", &head, &entry("com.wrong", "1", Some("wrong"))).unwrap_err();
    assert!(e.to_string().contains("com.other"), "{e}");
    assert!(materialize(&paths, "s", &head, &entry("com.m", "1", Some("missing"))).is_err());

    assert_eq!(entries(&paths.materialized("s")), ["a"]);
    assert!(matches_catalog(
      &paths.materialized("s").join("a"),
      &entry("com.a", "1", None)
    ));
  }

  #[test]
  fn replace_stays_inside_and_rolls_back() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("root");
    let staged = root.join("staged");
    let target = root.join("target");
    fs::create_dir_all(&staged).unwrap();
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("old"), "").unwrap();

    assert!(replace_directory(&root, &staged, &tmp.path().join("outside")).is_err());
    assert!(replace_directory(&root, &tmp.path().join("x"), &target).is_err());

    // the staged directory is gone: the rename fails and the old copy is back
    let e = replace_directory(&root, &root.join("missing"), &target);
    assert!(e.is_err());
    assert!(target.join("old").exists());
    assert_eq!(entries(&root), ["staged", "target"]);

    fs::write(staged.join("new"), "").unwrap();
    replace_directory(&root, &staged, &target).unwrap();
    assert!(target.join("new").exists() && !target.join("old").exists());
    assert_eq!(entries(&root), ["target"]);
  }
}
