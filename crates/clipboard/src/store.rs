//! The history on disk: one SQLite database with a full text index, images beside it

use std::{
  collections::hash_map::DefaultHasher,
  hash::{Hash, Hasher},
  path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

/// What an entry holds, for the type filter
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
  Text,
  Image,
  Link,
  Color,
}

impl Kind {
  pub fn as_str(self) -> &'static str {
    match self {
      Kind::Text => "text",
      Kind::Image => "image",
      Kind::Link => "link",
      Kind::Color => "color",
    }
  }

  fn parse(s: &str) -> Self {
    match s {
      "image" => Kind::Image,
      "link" => Kind::Link,
      "color" => Kind::Color,
      _ => Kind::Text,
    }
  }

  /// A link or a colour when the text is just that
  pub fn of_text(text: &str) -> Self {
    let t = text.trim();
    let function = !t.contains('\n')
      && ["rgb(", "rgba(", "hsl(", "hsla(", "oklch("]
        .iter()
        .any(|f| t.starts_with(f) && t.ends_with(')'));
    if function {
      return Kind::Color;
    }
    if t.contains(char::is_whitespace) {
      return Kind::Text;
    }
    if t.starts_with("http://") || t.starts_with("https://") {
      return Kind::Link;
    }
    let hex = t.strip_prefix('#').is_some_and(|h| {
      matches!(h.len(), 3 | 4 | 6 | 8) && h.chars().all(|c| c.is_ascii_hexdigit())
    });
    match hex {
      true => Kind::Color,
      false => Kind::Text,
    }
  }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
  pub id: i64,
  pub kind: Kind,
  /// The text, cut to a preview in lists; None for images
  pub text: Option<String>,
  pub mime: String,
  /// The image file
  pub path: Option<PathBuf>,
  pub size: u64,
  pub pinned: bool,
  /// Unix milliseconds
  pub created: u64,
  pub used: u64,
}

pub struct Store {
  db: Connection,
  dir: PathBuf,
}

const PREVIEW: usize = 2000;

/// Long side of a thumbnail in pixels
const THUMB_SIDE: u32 = 256;

/// `<hash>.png` -> `<hash>.thumb.png`
pub fn thumb_path(path: &Path) -> PathBuf {
  path.with_extension("thumb.png")
}

fn is_thumb(path: &Path) -> bool {
  path
    .file_name()
    .is_some_and(|n| n.to_string_lossy().ends_with(".thumb.png"))
}

/// The size to scale to, None when the image is small enough as it is
fn thumb_size(w: u32, h: u32) -> Option<(u32, u32)> {
  if w.max(h) <= THUMB_SIDE || w == 0 || h == 0 {
    return None;
  }
  let scale =
    |a: u32, b: u32| ((u64::from(a) * u64::from(THUMB_SIDE) / u64::from(b)) as u32).max(1);
  Some(match w >= h {
    true => (THUMB_SIDE, scale(h, w)),
    false => (scale(w, h), THUMB_SIDE),
  })
}

/// Images that were tried this session, thumbnail or not, so a failure is never retried
static TRIED: std::sync::Mutex<Option<std::collections::HashSet<PathBuf>>> =
  std::sync::Mutex::new(None);

/// Whether a thumbnail should be generated for `path`: none yet and not tried before
fn should_generate(path: &Path, tried: &mut std::collections::HashSet<PathBuf>) -> bool {
  !is_thumb(path) && !thumb_path(path).exists() && tried.insert(path.to_path_buf())
}

fn make_thumb(path: &Path) -> Result<()> {
  let img = image::open(path)?;
  let Some((w, h)) = thumb_size(img.width(), img.height()) else {
    return Ok(());
  };
  let out = thumb_path(path);
  let tmp = out.with_extension("tmp");
  img
    .resize_exact(w, h, image::imageops::FilterType::Triangle)
    .save_with_format(&tmp, image::ImageFormat::Png)?;
  std::fs::rename(&tmp, &out)?;
  Ok(())
}

/// The thumbnail when there is one, else the original; starts generating a missing one in the
/// background (once per session). Never decodes on the calling thread.
pub fn display_path(path: &Path) -> PathBuf {
  let thumb = thumb_path(path);
  if thumb.exists() {
    return thumb;
  }
  let start = {
    let mut tried = TRIED.lock().unwrap_or_else(|e| e.into_inner());
    should_generate(path, tried.get_or_insert_with(Default::default))
  };
  if start {
    let path = path.to_path_buf();
    std::thread::spawn(move || {
      if let Err(e) = make_thumb(&path) {
        tracing::debug!("no thumbnail for {}: {e}", path.display());
      }
    });
  }
  path.to_path_buf()
}

/// Thumbnails whose image is gone
fn remove_orphan_thumbs(dir: &Path) -> Result<()> {
  let files: Vec<PathBuf> = std::fs::read_dir(dir)?
    .flatten()
    .map(|f| f.path())
    .collect();
  for thumb in files.iter().filter(|p| is_thumb(p)) {
    let stem = thumb
      .file_name()
      .map(|n| n.to_string_lossy().replace(".thumb.png", "."));
    let has_image = files.iter().any(|f| {
      !is_thumb(f)
        && f.file_name().is_some_and(|n| {
          let n = n.to_string_lossy();
          stem.as_deref().is_some_and(|s| n.starts_with(s)) && !n.ends_with(".tmp")
        })
    });
    if !has_image {
      let _ = std::fs::remove_file(thumb);
    }
  }
  Ok(())
}

fn hash(bytes: &[u8]) -> String {
  let mut hasher = DefaultHasher::new();
  bytes.hash(&mut hasher);
  bytes.len().hash(&mut hasher);
  format!("{:016x}", hasher.finish())
}

fn now() -> i64 {
  std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map_or(0, |d| d.as_millis() as i64)
}

/// FTS5 query: every word as a quoted prefix
fn fts_query(query: &str) -> String {
  query
    .split_whitespace()
    .map(|w| format!("\"{}\"*", w.replace('"', "\"\"")))
    .collect::<Vec<_>>()
    .join(" ")
}

impl Store {
  /// `~/.local/share/corona/clipboard`, only the user may read it
  pub fn default_dir() -> Result<PathBuf> {
    Ok(
      dirs::data_dir()
        .context("no data directory")?
        .join("corona")
        .join("clipboard"),
    )
  }

  pub fn open(dir: &Path) -> Result<Self> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let db = Connection::open(dir.join("history.db"))?;
    db.execute_batch(
      "PRAGMA journal_mode = WAL;
      PRAGMA synchronous = NORMAL;
      CREATE TABLE IF NOT EXISTS entries (
        id INTEGER PRIMARY KEY,
        kind TEXT NOT NULL,
        text TEXT,
        mime TEXT NOT NULL,
        hash TEXT NOT NULL UNIQUE,
        path TEXT,
        size INTEGER NOT NULL,
        pinned INTEGER NOT NULL DEFAULT 0,
        created INTEGER NOT NULL,
        used INTEGER NOT NULL
      );
      CREATE INDEX IF NOT EXISTS entries_used ON entries (pinned DESC, used DESC);
      CREATE VIRTUAL TABLE IF NOT EXISTS entries_fts USING fts5(text, content='entries', content_rowid='id');
      CREATE TRIGGER IF NOT EXISTS entries_ai AFTER INSERT ON entries BEGIN
        INSERT INTO entries_fts(rowid, text) VALUES (new.id, new.text);
      END;
      CREATE TRIGGER IF NOT EXISTS entries_ad AFTER DELETE ON entries BEGIN
        INSERT INTO entries_fts(entries_fts, rowid, text) VALUES ('delete', old.id, old.text);
      END;",
    )?;
    Ok(Self {
      db,
      dir: dir.to_path_buf(),
    })
  }

  pub fn dir(&self) -> &Path {
    &self.dir
  }

  /// Adds text, or moves the same text to the top again
  pub fn add_text(&self, text: &str, mime: &str) -> Result<i64> {
    let kind = Kind::of_text(text);
    self.upsert(
      kind,
      Some(text),
      mime,
      hash(text.as_bytes()),
      None,
      text.len() as u64,
    )
  }

  /// Adds an image, kept as a file named after its hash
  #[cfg(test)]
  pub fn add_image(&self, bytes: &[u8], mime: &str) -> Result<i64> {
    let (hash, path) = Self::write_image(&self.dir, bytes, mime)?;
    self.add_image_file(hash, &path, mime, bytes.len() as u64)
  }

  /// Hashes and writes an image file into `dir`, for a thread that is not the database's
  pub fn write_image(dir: &Path, bytes: &[u8], mime: &str) -> Result<(String, PathBuf)> {
    let hash = hash(bytes);
    let ext = mime.rsplit('/').next().unwrap_or("png");
    let path = dir.join(format!("{hash}.{ext}"));
    if !path.exists() {
      // renamed into place, so a cut-off write is never mistaken for the image
      let tmp = dir.join(format!("{hash}.tmp"));
      std::fs::write(&tmp, bytes)?;
      std::fs::rename(&tmp, &path)?;
    }
    // off the caller's way; a failure keeps the original
    if !thumb_path(&path).exists() {
      let path = path.clone();
      std::thread::spawn(move || {
        if let Err(e) = make_thumb(&path) {
          tracing::debug!("no thumbnail for {}: {e}", path.display());
        }
      });
    }
    Ok((hash, path))
  }

  /// Adds an image that [`Store::write_image`] has written
  pub fn add_image_file(&self, hash: String, path: &Path, mime: &str, size: u64) -> Result<i64> {
    // a delete of the same image may have removed the file since it was written
    anyhow::ensure!(path.exists(), "image file is gone: {}", path.display());
    self.upsert(Kind::Image, None, mime, hash, Some(path), size)
  }

  fn upsert(
    &self,
    kind: Kind,
    text: Option<&str>,
    mime: &str,
    hash: String,
    path: Option<&Path>,
    size: u64,
  ) -> Result<i64> {
    // one statement, so another shell on the same database cannot slip in between
    Ok(self.db.query_row(
      "INSERT INTO entries (kind, text, mime, hash, path, size, created, used)
       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
       ON CONFLICT (hash) DO UPDATE SET used = excluded.used
       RETURNING id",
      params![
        kind.as_str(),
        text,
        mime,
        hash,
        path.map(|p| p.to_string_lossy().to_string()),
        size as i64,
        now()
      ],
      |r| r.get(0),
    )?)
  }

  fn entry(row: &rusqlite::Row) -> rusqlite::Result<Entry> {
    Ok(Entry {
      id: row.get(0)?,
      kind: Kind::parse(&row.get::<_, String>(1)?),
      text: row.get(2)?,
      mime: row.get(3)?,
      path: row.get::<_, Option<String>>(4)?.map(PathBuf::from),
      size: row.get::<_, i64>(5)? as u64,
      pinned: row.get::<_, i64>(6)? != 0,
      created: row.get::<_, i64>(7)? as u64,
      used: row.get::<_, i64>(8)? as u64,
    })
  }

  /// Pinned first, then newest; `query` searches the text, `kind` filters
  pub fn list(&self, query: &str, kind: Option<Kind>, limit: usize) -> Result<Vec<Entry>> {
    let columns = format!(
      "e.id, e.kind, substr(e.text, 1, {PREVIEW}), e.mime, e.path, e.size, e.pinned, e.created, e.used"
    );
    let kind = kind.map(Kind::as_str);
    let rows = match query.trim().is_empty() {
      true => {
        let mut statement = self.db.prepare(&format!(
          "SELECT {columns} FROM entries e WHERE (?1 IS NULL OR e.kind = ?1)
           ORDER BY e.pinned DESC, e.used DESC LIMIT ?2"
        ))?;
        statement
          .query_map(params![kind, limit as i64], Self::entry)?
          .collect::<rusqlite::Result<Vec<_>>>()?
      }
      false => {
        let mut statement = self.db.prepare(&format!(
          "SELECT {columns} FROM entries e JOIN entries_fts f ON f.rowid = e.id
           WHERE entries_fts MATCH ?1 AND (?2 IS NULL OR e.kind = ?2)
           ORDER BY e.pinned DESC, f.rank, e.used DESC LIMIT ?3"
        ))?;
        statement
          .query_map(params![fts_query(query), kind, limit as i64], Self::entry)?
          .collect::<rusqlite::Result<Vec<_>>>()?
      }
    };
    Ok(rows)
  }

  /// The whole entry, text not cut
  pub fn get(&self, id: i64) -> Result<Option<Entry>> {
    Ok(
      self
        .db
        .query_row(
          "SELECT id, kind, text, mime, path, size, pinned, created, used FROM entries WHERE id = ?1",
          [id],
          Self::entry,
        )
        .optional()?,
    )
  }

  /// The `offset`th newest text, 0 the latest
  pub fn nth_text(&self, offset: usize) -> Result<Option<String>> {
    Ok(
      self
        .db
        .query_row(
          "SELECT text FROM entries WHERE text IS NOT NULL ORDER BY used DESC LIMIT 1 OFFSET ?1",
          [offset as i64],
          |r| r.get(0),
        )
        .optional()?,
    )
  }

  pub fn set_pinned(&self, id: i64, pinned: bool) -> Result<()> {
    self.db.execute(
      "UPDATE entries SET pinned = ?1 WHERE id = ?2",
      params![pinned, id],
    )?;
    Ok(())
  }

  pub fn touch(&self, id: i64) -> Result<()> {
    self.db.execute(
      "UPDATE entries SET used = ?1 WHERE id = ?2",
      params![now(), id],
    )?;
    Ok(())
  }

  pub fn delete(&self, id: i64) -> Result<()> {
    // files go only once the rows are gone
    let path: Option<Option<String>> = self
      .db
      .query_row(
        "DELETE FROM entries WHERE id = ?1 RETURNING path",
        [id],
        |r| r.get(0),
      )
      .optional()?;
    if let Some(path) = path.flatten() {
      let _ = std::fs::remove_file(thumb_path(Path::new(&path)));
      let _ = std::fs::remove_file(path);
    }
    Ok(())
  }

  /// Everything but the pinned entries
  pub fn clear(&self) -> Result<usize> {
    self.delete_where("pinned = 0", params![])
  }

  /// Images older than `days`, pinned ones stay
  pub fn prune_images(&self, days: u32) -> Result<usize> {
    // leftovers of writes that never finished
    for file in std::fs::read_dir(&self.dir)?.flatten() {
      if file.path().extension().is_some_and(|e| e == "tmp") {
        let _ = std::fs::remove_file(file.path());
      }
    }
    let _ = remove_orphan_thumbs(&self.dir);
    let before = now() - i64::from(days) * 86_400_000;
    self.delete_where(
      "kind = 'image' AND pinned = 0 AND used < ?1",
      params![before],
    )
  }

  fn delete_where(&self, condition: &str, args: impl rusqlite::Params + Clone) -> Result<usize> {
    // files go only once the rows are gone
    let mut statement = self.db.prepare(&format!(
      "DELETE FROM entries WHERE {condition} RETURNING path"
    ))?;
    let paths: Vec<Option<String>> = statement
      .query_map(args, |r| r.get(0))?
      .collect::<rusqlite::Result<_>>()?;
    for path in paths.iter().flatten() {
      let _ = std::fs::remove_file(thumb_path(Path::new(path)));
      let _ = std::fs::remove_file(path);
    }
    Ok(paths.len())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn fts_query_quotes() {
    assert_eq!(fts_query("a \"b\" c*"), "\"a\"* \"\"\"b\"\"\"* \"c*\"*");
    assert_eq!(fts_query("  "), "");
  }

  #[test]
  fn kinds() {
    assert_eq!(Kind::of_text("https://example.com/a"), Kind::Link);
    assert_eq!(Kind::of_text("#3a3a8c"), Kind::Color);
    assert_eq!(Kind::of_text("rgb(1, 2, 3)"), Kind::Color);
    assert_eq!(Kind::of_text("rgb(1,2,3)"), Kind::Color);
    assert_eq!(Kind::of_text("hello world"), Kind::Text);
    assert_eq!(Kind::of_text("#zzz"), Kind::Text);
  }

  #[test]
  fn history() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let a = store.add_text("hello world", "text/plain").unwrap();
    let b = store.add_text("https://example.com", "text/plain").unwrap();
    let img = store.add_image(b"\x89PNG fake", "image/png").unwrap();
    // the same text again moves to the top, no duplicate
    std::thread::sleep(std::time::Duration::from_millis(2));
    assert_eq!(store.add_text("hello world", "text/plain").unwrap(), a);
    let ids: Vec<i64> = store
      .list("", None, 10)
      .unwrap()
      .iter()
      .map(|e| e.id)
      .collect();
    assert_eq!(ids.len(), 3);
    assert_eq!(ids[0], a);

    // full text search with prefixes, and the type filter
    let found = store.list("hel wor", None, 10).unwrap();
    assert_eq!(found.iter().map(|e| e.id).collect::<Vec<_>>(), [a]);
    assert!(store.list("exam", Some(Kind::Text), 10).unwrap().is_empty());
    assert_eq!(store.list("", Some(Kind::Link), 10).unwrap()[0].id, b);
    assert_eq!(store.list("", Some(Kind::Image), 10).unwrap()[0].id, img);
    assert_eq!(store.nth_text(0).unwrap().as_deref(), Some("hello world"));
    assert_eq!(
      store.nth_text(1).unwrap().as_deref(),
      Some("https://example.com")
    );

    // pins survive clearing, the rest goes with its files
    store.set_pinned(b, true).unwrap();
    let image = store.get(img).unwrap().unwrap().path.unwrap();
    assert!(image.exists());
    assert_eq!(store.clear().unwrap(), 2);
    assert!(!image.exists());
    let left = store.list("", None, 10).unwrap();
    assert_eq!(left.len(), 1);
    assert!(left[0].pinned);
    // and the search index followed
    assert!(store.list("hello", None, 10).unwrap().is_empty());
    store.delete(b).unwrap();
    assert!(store.list("", None, 10).unwrap().is_empty());
  }

  #[test]
  fn thumb_sizes_and_names() {
    assert_eq!(thumb_size(200, 100), None);
    assert_eq!(thumb_size(0, 900), None);
    assert_eq!(thumb_size(1024, 512), Some((256, 128)));
    assert_eq!(thumb_size(512, 2048), Some((64, 256)));
    assert_eq!(thumb_size(10000, 10), Some((256, 1)));
    let p = Path::new("/d/abc.png");
    assert_eq!(thumb_path(p), Path::new("/d/abc.thumb.png"));
    assert!(is_thumb(&thumb_path(p)) && !is_thumb(p));
  }

  #[test]
  fn lazy_decision_and_failures() {
    let dir = tempfile::tempdir().unwrap();
    let img = dir.path().join("a.png");
    std::fs::write(&img, b"not an image").unwrap();
    let mut tried = Default::default();
    assert!(should_generate(&img, &mut tried));
    // never twice in a session, even though it failed
    assert!(make_thumb(&img).is_err());
    assert!(!should_generate(&img, &mut tried));
    // never when one exists
    let other = dir.path().join("b.png");
    std::fs::write(thumb_path(&other), b"x").unwrap();
    assert!(!should_generate(&other, &mut tried));
    assert!(!thumb_path(&img).exists());
  }

  #[test]
  fn thumbnail_lifecycle() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::RgbImage::new(600, 300)
      .write_to(&mut bytes, image::ImageFormat::Png)
      .unwrap();
    let (_, path) = Store::write_image(dir.path(), bytes.get_ref(), "image/png").unwrap();
    make_thumb(&path).unwrap();
    let thumb = thumb_path(&path);
    assert_eq!(image::image_dimensions(&thumb).unwrap(), (256, 128));
    assert_eq!(display_path(&path), thumb);
    // an orphan goes on prune, a live thumb stays
    let orphan = dir.path().join("dead.thumb.png");
    std::fs::write(&orphan, b"x").unwrap();
    store.prune_images(30).unwrap();
    assert!(!orphan.exists() && thumb.exists());
    // deleting the entry takes the thumbnail along
    let id = store
      .add_image_file("h".into(), &path, "image/png", 1)
      .unwrap();
    store.delete(id).unwrap();
    assert!(!path.exists() && !thumb.exists());
  }

  #[test]
  fn quotes_in_queries() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    store.add_text("say \"hi\"", "text/plain").unwrap();
    assert_eq!(store.list("\"hi", None, 10).unwrap().len(), 1);
  }
}
