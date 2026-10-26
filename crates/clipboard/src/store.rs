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
  fn as_str(self) -> &'static str {
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
    if t.contains(char::is_whitespace) {
      return Kind::Text;
    }
    if t.starts_with("http://") || t.starts_with("https://") {
      return Kind::Link;
    }
    let hex = t.strip_prefix('#').is_some_and(|h| {
      matches!(h.len(), 3 | 4 | 6 | 8) && h.chars().all(|c| c.is_ascii_hexdigit())
    });
    let function = ["rgb(", "rgba(", "hsl(", "hsla(", "oklch("]
      .iter()
      .any(|f| t.starts_with(f) && t.ends_with(')'));
    match hex || function {
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
  /// Unix seconds
  pub created: u64,
  pub used: u64,
}

pub struct Store {
  db: Connection,
  dir: PathBuf,
}

const PREVIEW: usize = 2000;

fn hash(bytes: &[u8]) -> String {
  let mut hasher = DefaultHasher::new();
  bytes.hash(&mut hasher);
  bytes.len().hash(&mut hasher);
  format!("{:016x}", hasher.finish())
}

fn now() -> i64 {
  std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map_or(0, |d| d.as_secs() as i64)
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
  pub fn add_image(&self, bytes: &[u8], mime: &str) -> Result<i64> {
    let hash = hash(bytes);
    let ext = mime.rsplit('/').next().unwrap_or("png");
    let path = self.dir.join(format!("{hash}.{ext}"));
    if !path.exists() {
      std::fs::write(&path, bytes)?;
    }
    self.upsert(
      Kind::Image,
      None,
      mime,
      hash,
      Some(&path),
      bytes.len() as u64,
    )
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
    if let Some(path) = self.get(id)?.and_then(|e| e.path) {
      let _ = std::fs::remove_file(path);
    }
    self.db.execute("DELETE FROM entries WHERE id = ?1", [id])?;
    Ok(())
  }

  /// Everything but the pinned entries
  pub fn clear(&self) -> Result<usize> {
    self.delete_where("pinned = 0", params![])
  }

  /// Images older than `days`, pinned ones stay
  pub fn prune_images(&self, days: u32) -> Result<usize> {
    let before = now() - i64::from(days) * 86_400;
    self.delete_where(
      "kind = 'image' AND pinned = 0 AND used < ?1",
      params![before],
    )
  }

  fn delete_where(&self, condition: &str, args: impl rusqlite::Params + Clone) -> Result<usize> {
    let mut statement = self.db.prepare(&format!(
      "SELECT path FROM entries WHERE {condition} AND path IS NOT NULL"
    ))?;
    let paths: Vec<String> = statement
      .query_map(args.clone(), |r| r.get(0))?
      .collect::<rusqlite::Result<_>>()?;
    for path in paths {
      let _ = std::fs::remove_file(path);
    }
    Ok(
      self
        .db
        .execute(&format!("DELETE FROM entries WHERE {condition}"), args)?,
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn kinds() {
    assert_eq!(Kind::of_text("https://example.com/a"), Kind::Link);
    assert_eq!(Kind::of_text("#3a3a8c"), Kind::Color);
    assert_eq!(Kind::of_text("rgb(1, 2, 3)"), Kind::Text);
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
    std::thread::sleep(std::time::Duration::from_millis(1100));
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
  fn quotes_in_queries() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    store.add_text("say \"hi\"", "text/plain").unwrap();
    assert_eq!(store.list("\"hi", None, 10).unwrap().len(), 1);
  }
}
