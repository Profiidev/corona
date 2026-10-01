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
