//! `localStorage` (a JSON file per plugin, written atomically off the JS thread) and
//! `sessionStorage` (memory). Runtimes of one plugin share them.

use std::{
  cell::{Cell, RefCell},
  collections::{BTreeMap, HashMap},
  io::Write,
  path::{Path, PathBuf},
  rc::{Rc, Weak},
  sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
  },
};

use rquickjs::{Coerced, Ctx, Exception, JsLifetime, Promise, Result, class::Trace};

use super::io::{Fail, Null, Res, blocking, promise};
use crate::{PluginRef, runtime::caps::CapabilityError};

/// Total bytes of keys and values a store may hold
const QUOTA: usize = 5 * 1024 * 1024;

/// Revisions are global so stores of one file, made after a reload, still order their writes
static REVISION: AtomicU64 = AtomicU64::new(0);

/// The revision on disk per file, also the lock that orders its writes, shared by every
/// store of that file
static WRITTEN: Mutex<BTreeMap<PathBuf, Arc<Mutex<u64>>>> = Mutex::new(BTreeMap::new());

fn written(file: &Path) -> Arc<Mutex<u64>> {
  WRITTEN
    .lock()
    .unwrap()
    .entry(file.into())
    .or_default()
    .clone()
}

pub(crate) struct Store {
  map: RefCell<BTreeMap<String, String>>,
  /// Bytes of keys and values in `map`
  used: Cell<usize>,
  /// Where it persists; `None` for session storage
  file: Option<PathBuf>,
  revision: Cell<u64>,
  /// A write of the latest revision is queued for the end of this tick
  scheduled: Cell<bool>,
  /// The revision on disk, also the lock that orders writes
  written: Arc<Mutex<u64>>,
}

thread_local! {
  static FILES: RefCell<HashMap<PathBuf, Weak<Store>>> = RefCell::default();
  static SESSIONS: RefCell<HashMap<String, Rc<Store>>> = RefCell::default();
}

impl Store {
  fn new(file: Option<PathBuf>, map: BTreeMap<String, String>) -> Rc<Self> {
    Rc::new(Self {
      used: Cell::new(map.iter().map(|(k, v)| k.len() + v.len()).sum()),
      map: RefCell::new(map),
      written: file.as_deref().map(written).unwrap_or_default(),
      file,
      revision: Cell::new(0),
      scheduled: Cell::new(false),
    })
  }

  /// The store persisted in `file`, shared while any runtime holds it
  fn file(file: PathBuf) -> Rc<Self> {
    FILES.with_borrow_mut(|files| {
      if let Some(store) = files.get(&file).and_then(Weak::upgrade) {
        return store;
      }
      // an earlier store's write may still be in flight
      let settled = written(&file);
      let _settled = settled.lock().unwrap();
      let map = match std::fs::read(&file) {
        Ok(data) => serde_json::from_slice(&data).unwrap_or_else(|e| {
          tracing::warn!("{} is not storage, starting empty: {e}", file.display());
          BTreeMap::new()
        }),
        Err(_) => BTreeMap::new(),
      };
      let store = Self::new(Some(file.clone()), map);
      files.insert(file, Rc::downgrade(&store));
      store
    })
  }

  fn session(plugin: &str) -> Rc<Self> {
    SESSIONS.with_borrow_mut(|s| {
      s.entry(plugin.into())
        .or_insert_with(|| Self::new(None, BTreeMap::new()))
        .clone()
    })
  }

  /// Persists the current revision on a blocking thread
  fn persist(&self) -> Option<impl Future<Output = Res<()>> + use<>> {
    let file = self.file.clone()?;
    let revision = self.revision.get();
    let json = serde_json::to_vec(&*self.map.borrow()).expect("strings");
    let written = self.written.clone();
    Some(blocking(move || {
      let mut on_disk = written.lock().unwrap();
      if *on_disk >= revision {
        return Ok(());
      }
      write(&file, &json, revision)
        .map_err(|e| Fail::Error(format!("writing {} failed: {e}", file.display())))?;
      *on_disk = revision;
      Ok(())
    }))
  }

  /// After every change; the changes of one tick share a write. A failed write parks until
  /// `flush()` retries it.
  fn changed(self: &Rc<Self>) {
    self
      .revision
      .set(REVISION.fetch_add(1, Ordering::Relaxed) + 1);
    if self.file.is_none() || self.scheduled.replace(true) {
      return;
    }
    let store = self.clone();
    let tick = crate::app().foreground_executor().spawn(async move {
      store.scheduled.set(false);
      if let Some(write) = store.persist() {
        super::io::detach(async move {
          if let Err(Fail::Error(e)) = write.await {
            tracing::warn!("storage not saved: {e}");
          }
        });
      }
    });
    tick.detach();
  }
}

/// Atomically: a temporary file renamed over the target
fn write(file: &Path, json: &[u8], revision: u64) -> std::io::Result<()> {
  if let Some(dir) = file.parent() {
    std::fs::create_dir_all(dir)?;
  }
  let tmp = file.with_extension(format!("json.{revision}.tmp"));
  (|| {
    let mut f = std::fs::File::create(&tmp)?;
    f.write_all(json)?;
    f.sync_all()?;
    std::fs::rename(&tmp, file)
  })()
  .inspect_err(|_| {
    let _ = std::fs::remove_file(&tmp);
  })
}

/// The Web Storage API
#[derive(Trace, JsLifetime)]
#[rquickjs::class]
pub struct Storage {
  #[qjs(skip_trace)]
  store: Rc<Store>,
  #[qjs(skip_trace)]
  granted: bool,
}

impl Storage {
  /// In memory for a plugin without a data directory
  pub(crate) fn local(plugin: &PluginRef) -> Self {
    let store = match plugin.data_dir.as_os_str().is_empty() {
      true => Store::new(None, BTreeMap::new()),
      false => Store::file(plugin.data_dir.join("localStorage.json")),
    };
    Self {
      store,
      granted: plugin.capabilities.has_storage(),
    }
  }

  pub(crate) fn session(plugin: &PluginRef) -> Self {
    Self {
      store: Store::session(&plugin.id),
      granted: true,
    }
  }

  fn map(&self, ctx: &Ctx<'_>) -> Result<&RefCell<BTreeMap<String, String>>> {
    match self.granted {
      true => Ok(&self.store.map),
      false => Err(Exception::throw_type(
        ctx,
        &CapabilityError::StorageDenied.to_string(),
      )),
    }
  }
}

#[rquickjs::methods]
impl Storage {
  #[qjs(get)]
  pub fn length(&self, ctx: Ctx<'_>) -> Result<u32> {
    Ok(self.map(&ctx)?.borrow().len() as u32)
  }

  pub fn key(&self, ctx: Ctx<'_>, index: u32) -> Result<Null<String>> {
    Ok(Null(
      self.map(&ctx)?.borrow().keys().nth(index as usize).cloned(),
    ))
  }

  #[qjs(rename = "getItem")]
  pub fn get_item(&self, ctx: Ctx<'_>, key: Coerced<String>) -> Result<Null<String>> {
    Ok(Null(self.map(&ctx)?.borrow().get(&key.0).cloned()))
  }

  #[qjs(rename = "setItem")]
  pub fn set_item(&self, ctx: Ctx<'_>, key: Coerced<String>, value: Coerced<String>) -> Result<()> {
    let mut map = self.map(&ctx)?.borrow_mut();
    let size = |k: &str, v: &str| k.len() + v.len();
    let old = map.get(&key.0).map_or(0, |v| size(&key.0, v));
    let used = self.store.used.get() - old + size(&key.0, &value.0);
    if used > QUOTA {
      return Err(Exception::throw_message(
        &ctx,
        "QuotaExceededError: storage is limited to 5 MiB",
      ));
    }
    map.insert(key.0, value.0);
    self.store.used.set(used);
    drop(map);
    self.store.changed();
    Ok(())
  }

  #[qjs(rename = "removeItem")]
  pub fn remove_item(&self, ctx: Ctx<'_>, key: Coerced<String>) -> Result<()> {
    let removed = self.map(&ctx)?.borrow_mut().remove(&key.0);
    if let Some(v) = removed {
      self
        .store
        .used
        .set(self.store.used.get() - key.0.len() - v.len());
      self.store.changed();
    }
    Ok(())
  }

  pub fn clear(&self, ctx: Ctx<'_>) -> Result<()> {
    self.map(&ctx)?.borrow_mut().clear();
    self.store.used.set(0);
    self.store.changed();
    Ok(())
  }

  /// Resolves once the last change is on disk; retries a failed write.
  pub fn flush<'js>(&self, ctx: Ctx<'js>) -> Result<Promise<'js>> {
    self.map(&ctx)?;
    let write = self.store.persist();
    promise(&ctx, async move {
      match write {
        Some(write) => write.await,
        None => Ok(()),
      }
    })
  }
}
