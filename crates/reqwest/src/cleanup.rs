use std::{
  fs, io,
  path::{Path, PathBuf},
  time::{Duration, SystemTime},
};

use crate::store::{path, read_meta, remove_entry};

/// above this the least recently used entries go
pub(crate) const MAX_SIZE: u64 = 1024 * 1024 * 1024;
/// a temp file older than this belongs to a write that died, not one in progress
const STALE_TEMP: Duration = Duration::from_secs(60 * 60);

pub(crate) fn cleanup(dir: &Path, max_size: u64, now: u64) -> io::Result<()> {
  remove_broken(dir, now)?;
  shrink_to(dir, max_size)
}

fn extension(file: &Path) -> Option<&str> {
  file.extension().and_then(|e| e.to_str())
}

/// expired entries, half entries and temp files of dead writes
fn remove_broken(dir: &Path, now: u64) -> io::Result<()> {
  for entry in fs::read_dir(dir)? {
    let file = entry?.path();
    match extension(&file) {
      Some("meta") => {
        let expired = read_meta(&file).is_none_or(|meta| meta.expires <= now);
        if expired || !path(&file, "body").exists() {
          remove_entry(&file);
        }
      }
      Some("body") if !path(&file, "meta").exists() => remove_entry(&file),
      Some("tmp") => {
        let age = fs::metadata(&file)
          .and_then(|m| m.modified())
          .ok()
          .and_then(|modified| modified.elapsed().ok());
        if age.is_some_and(|age| age > STALE_TEMP) {
          fs::remove_file(&file).ok();
        }
      }
      _ => {}
    }
  }
  Ok(())
}

/// removes the least recently used entries until the rest fit `max_size`
fn shrink_to(dir: &Path, max_size: u64) -> io::Result<()> {
  let mut entries: Vec<(SystemTime, u64, PathBuf)> = fs::read_dir(dir)?
    .filter_map(|entry| {
      let file = entry.ok()?.path();
      (extension(&file) == Some("body")).then_some(())?;
      let body = fs::metadata(&file).ok()?;
      let meta = fs::metadata(path(&file, "meta")).map_or(0, |m| m.len());
      Some((body.modified().ok()?, body.len() + meta, file))
    })
    .collect();
  let mut total: u64 = entries.iter().map(|(_, size, _)| size).sum();
  entries.sort_by_key(|(used, _, _)| *used);
  for (_, size, file) in entries {
    if total <= max_size {
      break;
    }
    remove_entry(&file);
    total -= size;
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use crate::{
    store::{Meta, write_entry},
    temp_dir,
  };

  use super::*;

  #[test]
  fn cleanup() {
    let dir = temp_dir("cleanup");
    let entry = |name: &str, expires: u64, body: &[u8]| {
      let meta = Meta {
        url: name.into(),
        expires,
        etag: None,
        last_modified: None,
        content_type: None,
      };
      write_entry(&dir.join(name), &meta, body).unwrap();
    };
    entry("expired", 50, b"x");
    entry("old", 500, &[0; 100]);
    std::thread::sleep(Duration::from_millis(20));
    entry("recent", 500, &[0; 100]);
    fs::write(dir.join("orphan.body"), b"x").unwrap();

    // room for one entry with its meta: the least recently used goes
    super::cleanup(&dir, 250, 100).unwrap();
    let mut left: Vec<_> = fs::read_dir(&dir)
      .unwrap()
      .map(|e| e.unwrap().file_name().into_string().unwrap())
      .collect();
    left.sort();
    assert_eq!(left, ["recent.body", "recent.meta"]);
    fs::remove_dir_all(dir).ok();
  }
}
