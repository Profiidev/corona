//! Small cached copies of the pictures in file rows, so a list never decodes the originals

use std::{
  collections::{HashMap, HashSet},
  hash::{Hash, Hasher},
  path::{Path, PathBuf},
  sync::Mutex,
  time::{Duration, SystemTime},
};

use gpui_kit::{App, AppContext};

/// Longest side of a thumbnail
const SIDE: u32 = 256;
/// Thumbnails not written for this long are deleted
const MAX_AGE: Duration = Duration::from_secs(30 * 24 * 3600);
/// In-memory entries kept; a longer session starts over
const MAX_KNOWN: usize = 4096;

/// Source picture to the picture a row should show (the thumbnail, or the source when small)
static READY: Mutex<Option<HashMap<PathBuf, PathBuf>>> = Mutex::new(None);
/// Pictures queued or tried this session, so a failure is never retried
static TRIED: Mutex<Option<HashSet<PathBuf>>> = Mutex::new(None);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
  m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Pictures the image crate decodes; vectors stay as they are, gpui draws them at row size
pub fn is_raster(path: &Path) -> bool {
  matches!(
    path
      .extension()
      .map(|e| e.to_string_lossy().to_lowercase())
      .as_deref(),
    Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp")
  )
}

/// The thumbnail of `src` once `prepare` has made it; no I/O
pub fn ready(src: &Path) -> Option<PathBuf> {
  lock(&READY).as_ref()?.get(src).cloned()
}

fn cache_dir() -> Option<PathBuf> {
  dirs::cache_dir().map(|d| d.join("corona").join("thumbs"))
}

/// Where the thumbnail of `src` lives; changes with the file's size and modification time
fn target(dir: &Path, src: &Path) -> Option<PathBuf> {
  let meta = std::fs::metadata(src).ok()?;
  let mut hasher = std::hash::DefaultHasher::new();
  src.hash(&mut hasher);
  meta.len().hash(&mut hasher);
  meta.modified().ok()?.hash(&mut hasher);
  Some(dir.join(format!("{:016x}.png", hasher.finish())))
}

/// Makes the thumbnail of `src` at `dst`; Ok(false) when the picture is small enough as it is
fn generate(src: &Path, dst: &Path) -> anyhow::Result<bool> {
  let (w, h) = image::image_dimensions(src)?;
  if w.max(h) <= SIDE {
    return Ok(false);
  }
  let thumb = image::open(src)?.thumbnail(SIDE, SIDE);
  let tmp = dst.with_extension("tmp");
  thumb.save_with_format(&tmp, image::ImageFormat::Png)?;
  std::fs::rename(&tmp, dst)?;
  Ok(true)
}

fn prune(dir: &Path) {
  let Ok(read) = std::fs::read_dir(dir) else {
    return;
  };
  for file in read.flatten() {
    let old = file
      .metadata()
      .and_then(|m| m.modified())
      .ok()
      .and_then(|t| SystemTime::now().duration_since(t).ok())
      .is_some_and(|age| age > MAX_AGE);
    if old {
      let _ = std::fs::remove_file(file.path());
    }
  }
}

/// Starts making thumbnails for the pictures among `paths` and redraws when they are done.
/// Rows show a plain icon until then. Cheap to call on every query.
pub fn prepare<'a>(paths: impl IntoIterator<Item = &'a Path>, cx: &App) {
  let Some(dir) = cache_dir() else {
    return;
  };
  let mut jobs = Vec::new();
  {
    let mut tried = lock(&TRIED);
    let tried = tried.get_or_insert_with(Default::default);
    if tried.len() > MAX_KNOWN {
      tried.clear();
      lock(&READY).take();
    }
    for src in paths.into_iter().filter(|p| is_raster(p)) {
      if !tried.insert(src.to_path_buf()) {
        continue;
      }
      let Some(dst) = target(&dir, src) else {
        continue;
      };
      match dst.exists() {
        true => {
          lock(&READY)
            .get_or_insert_with(Default::default)
            .insert(src.to_path_buf(), dst);
        }
        false => jobs.push((src.to_path_buf(), dst)),
      }
    }
  }
  if jobs.is_empty() {
    return;
  }
  let task = cx.background_spawn(async move {
    if std::fs::create_dir_all(&dir).is_ok() {
      prune(&dir);
    }
    let mut made = Vec::new();
    for (src, dst) in jobs {
      match generate(&src, &dst) {
        Ok(true) => made.push((src, dst)),
        // already small: show the picture itself
        Ok(false) => made.push((src.clone(), src)),
        Err(e) => tracing::debug!("no thumbnail for {}: {e}", src.display()),
      }
    }
    made
  });
  cx.spawn(async move |cx| {
    let made = task.await;
    if made.is_empty() {
      return;
    }
    lock(&READY)
      .get_or_insert_with(Default::default)
      .extend(made);
    cx.update(|cx| cx.refresh_windows());
  })
  .detach();
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn big_pictures_get_a_small_png_and_small_ones_none() {
    let dir = tempfile::tempdir().unwrap();
    let big = dir.path().join("big.png");
    image::RgbImage::new(1000, 500).save(&big).unwrap();
    let small = dir.path().join("small.png");
    image::RgbImage::new(64, 64).save(&small).unwrap();
    let out = dir.path().join("out.png");
    assert!(generate(&big, &out).unwrap());
    assert_eq!(image::image_dimensions(&out).unwrap(), (256, 128));
    assert!(!generate(&small, &dir.path().join("none.png")).unwrap());
    assert!(!dir.path().join("none.png").exists());
  }

  #[test]
  fn target_follows_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.png");
    std::fs::write(&file, b"1").unwrap();
    let first = target(dir.path(), &file).unwrap();
    assert_eq!(first, target(dir.path(), &file).unwrap());
    std::fs::write(&file, b"22").unwrap();
    assert_ne!(first, target(dir.path(), &file).unwrap());
    assert!(target(dir.path(), &dir.path().join("gone.png")).is_none());
  }

  #[test]
  fn only_raster_pictures_are_thumbnailed() {
    assert!(is_raster(Path::new("/a/B.JPG")));
    assert!(!is_raster(Path::new("/a/b.svg")));
    assert!(!is_raster(Path::new("/a/b.txt")));
  }
}
