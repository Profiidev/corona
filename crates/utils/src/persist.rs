use std::path::Path;

use anyhow::{Context, Result};
use serde::{Serialize, de::DeserializeOwned};
use tracing::warn;

/// The JSON at `path`, `None` when it is missing or broken
pub fn load_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
  let bytes = std::fs::read(path).ok()?;
  serde_json::from_slice(&bytes)
    .inspect_err(|e| warn!("ignoring {}: {e}", path.display()))
    .ok()
}

/// Writes `path` through a temp file next to it, so a crash keeps the old one
pub fn save_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
  let dir = path.parent().context("no parent directory")?;
  let name = path.file_name().context("no file name")?.to_string_lossy();
  std::fs::create_dir_all(dir)?;
  let tmp = dir.join(format!(".{name}.tmp"));
  std::fs::write(&tmp, serde_json::to_vec(value)?)?;
  std::fs::rename(tmp, path)?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn round_trips_and_ignores_broken() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a").join("value.json");
    assert_eq!(load_json::<Vec<u32>>(&path), None);
    save_json(&path, &vec![1, 2]).unwrap();
    assert_eq!(load_json(&path), Some(vec![1, 2]));
    std::fs::write(&path, b"{ broken").unwrap();
    assert_eq!(load_json::<Vec<u32>>(&path), None);
  }
}
