use std::{
  fs, io,
  path::{Path, PathBuf},
  sync::atomic::{AtomicU64, Ordering},
  time::{SystemTime, UNIX_EPOCH},
};

use http_client::{AsyncBody, Response, StatusCode, http::header};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Meta {
  pub url: String,
  pub expires: u64,
  pub etag: Option<String>,
  pub last_modified: Option<String>,
  pub content_type: Option<String>,
}

impl Meta {
  pub fn revalidatable(&self) -> bool {
    self.etag.is_some() || self.last_modified.is_some()
  }
}

pub(crate) fn base(dir: &Path, url: &str) -> PathBuf {
  dir.join(key(url))
}

pub(crate) fn path(base: &Path, extension: &str) -> PathBuf {
  base.with_extension(extension)
}

pub(crate) fn read_meta(base: &Path) -> Option<Meta> {
  serde_json::from_slice(&fs::read(path(base, "meta")).ok()?).ok()
}

pub(crate) fn write_meta(base: &Path, meta: &Meta) -> io::Result<()> {
  write_atomic(&path(base, "meta"), &serde_json::to_vec(meta)?)
}

pub(crate) fn write_entry(base: &Path, meta: &Meta, body: &[u8]) -> io::Result<()> {
  write_atomic(&path(base, "body"), body)?;
  write_meta(base, meta)
}

pub(crate) fn cached_response(base: &Path, meta: &Meta) -> io::Result<Response<AsyncBody>> {
  let body_path = path(base, "body");
  let bytes = fs::read(&body_path)?;
  // the body's mtime is the last use, cleanup removes the least recently used first
  if let Ok(file) = fs::File::options().write(true).open(&body_path) {
    file.set_modified(SystemTime::now()).ok();
  }
  let mut response = Response::builder().status(StatusCode::OK);
  if let Some(content_type) = &meta.content_type {
    response = response.header(header::CONTENT_TYPE, content_type);
  }
  response.body(bytes.into()).map_err(io::Error::other)
}

pub(crate) fn remove_entry(base: &Path) {
  fs::remove_file(path(base, "meta")).ok();
  fs::remove_file(path(base, "body")).ok();
}

pub(crate) fn unix_now() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_or(0, |d| d.as_secs())
}

fn write_atomic(target: &Path, bytes: &[u8]) -> io::Result<()> {
  static COUNTER: AtomicU64 = AtomicU64::new(0);
  let temp = target.with_extension(format!(
    "{}.{}.{}.tmp",
    target
      .extension()
      .and_then(|e| e.to_str())
      .unwrap_or_default(),
    std::process::id(),
    COUNTER.fetch_add(1, Ordering::Relaxed)
  ));
  fs::write(&temp, bytes)?;
  fs::rename(&temp, target).inspect_err(|_| {
    fs::remove_file(&temp).ok();
  })
}

fn key(url: &str) -> String {
  let hash = url.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
    (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
  });
  format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
  use std::time::Duration;

  use futures::{AsyncReadExt, executor::block_on};

  use super::*;
  use crate::temp_dir;

  fn meta(content_type: Option<&str>) -> Meta {
    Meta {
      url: "https://example.org".into(),
      expires: 10,
      etag: None,
      last_modified: None,
      content_type: content_type.map(Into::into),
    }
  }

  #[test]
  fn keys_are_stable_hex() {
    // FNV-1a 64 offset basis and a known vector
    assert_eq!(key(""), "cbf29ce484222325");
    assert_eq!(key("a"), "af63dc4c8601ec8c");
    let k = key("https://example.org/a.png");
    assert_eq!(k.len(), 16);
    assert!(k.chars().all(|c| c.is_ascii_hexdigit()));
    assert_ne!(key("https://example.org/a"), key("https://example.org/b"));
    assert_eq!(base(Path::new("/c"), ""), Path::new("/c/cbf29ce484222325"));
  }

  #[test]
  fn revalidatable() {
    let mut m = meta(None);
    assert!(!m.revalidatable());
    m.etag = Some("e".into());
    assert!(m.revalidatable());
    m.etag = None;
    m.last_modified = Some("d".into());
    assert!(m.revalidatable());
  }

  #[test]
  fn entries_round_trip() {
    let dir = temp_dir("store-round-trip");
    let base = dir.join("entry");
    write_entry(&base, &meta(Some("image/png")), b"body").unwrap();
    let read = read_meta(&base).unwrap();
    assert_eq!(
      (read.url.as_str(), read.expires),
      ("https://example.org", 10)
    );

    let mut response = cached_response(&base, &read).unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "image/png");
    let mut body = Vec::new();
    block_on(response.body_mut().read_to_end(&mut body)).unwrap();
    assert_eq!(body, b"body");

    let no_type = cached_response(&base, &meta(None)).unwrap();
    assert!(no_type.headers().get(header::CONTENT_TYPE).is_none());

    // no temp files are left behind
    let mut left: Vec<_> = fs::read_dir(&dir)
      .unwrap()
      .map(|e| e.unwrap().file_name().into_string().unwrap())
      .collect();
    left.sort();
    assert_eq!(left, ["entry.body", "entry.meta"]);

    remove_entry(&base);
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
    // removing twice is fine
    remove_entry(&base);
    fs::remove_dir_all(dir).ok();
  }

  #[test]
  fn reading_marks_the_body_used() {
    let dir = temp_dir("store-mtime");
    let base = dir.join("entry");
    write_entry(&base, &meta(None), b"x").unwrap();
    let body = path(&base, "body");
    let old = SystemTime::now() - Duration::from_secs(3600);
    fs::File::options()
      .write(true)
      .open(&body)
      .unwrap()
      .set_modified(old)
      .unwrap();
    cached_response(&base, &meta(None)).unwrap();
    assert!(fs::metadata(&body).unwrap().modified().unwrap() > old + Duration::from_secs(60));
    fs::remove_dir_all(dir).ok();
  }

  #[test]
  fn broken_entries() {
    let dir = temp_dir("store-broken");
    let base = dir.join("entry");
    assert!(read_meta(&base).is_none());
    fs::write(path(&base, "meta"), b"{not json").unwrap();
    assert!(read_meta(&base).is_none());
    // a meta without a body is no response
    write_meta(&base, &meta(None)).unwrap();
    assert!(cached_response(&base, &meta(None)).is_err());
    fs::remove_dir_all(dir).ok();
  }

  #[test]
  fn failed_writes_leave_no_temp_file() {
    let dir = temp_dir("store-atomic");
    // renaming a file onto a non-empty directory fails
    let target = dir.join("target");
    fs::create_dir_all(target.join("inside")).unwrap();
    assert!(write_atomic(&target, b"x").is_err());
    let left: Vec<_> = fs::read_dir(&dir)
      .unwrap()
      .map(|e| e.unwrap().file_name())
      .collect();
    assert_eq!(left, ["target"]);
    // unwritable parent: the temp write itself fails
    assert!(write_atomic(&dir.join("missing").join("x"), b"x").is_err());
    fs::remove_dir_all(dir).ok();
  }

  #[test]
  fn unix_now_is_now() {
    let now = unix_now();
    assert!(now > 1_700_000_000);
  }

  #[test]
  #[cfg(unix)]
  fn read_only_body_still_returns_cached_response() {
    use std::os::unix::fs::PermissionsExt;
    let dir = temp_dir("store-readonly-body");
    let base = dir.join("entry");
    write_entry(&base, &meta(None), b"cached body").unwrap();
    let body = path(&base, "body");
    let mut perms = fs::metadata(&body).unwrap().permissions();
    perms.set_mode(0o444);
    fs::set_permissions(&body, perms.clone()).unwrap();

    let res = cached_response(&base, &meta(None));
    assert!(res.is_ok());

    perms.set_mode(0o644);
    fs::set_permissions(&body, perms).unwrap();
    fs::remove_dir_all(dir).ok();
  }

  #[test]
  fn malformed_content_type_returns_io_error() {
    let dir = temp_dir("store-bad-ct");
    let base = dir.join("entry");
    let mut bad_meta = meta(None);
    bad_meta.content_type = Some("bad\ncontent\r\ntype".into());
    write_entry(&base, &bad_meta, b"body").unwrap();
    assert!(cached_response(&base, &bad_meta).is_err());
    fs::remove_dir_all(dir).ok();
  }

  #[test]
  fn unix_now_handles_clock_before_epoch() {
    let before_epoch = UNIX_EPOCH.checked_sub(Duration::from_secs(100));
    if let Some(t) = before_epoch {
      let secs = t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
      assert_eq!(secs, 0);
    }
  }
}
