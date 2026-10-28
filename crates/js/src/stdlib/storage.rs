//! `localStorage` (a JSON file per plugin, written atomically off the JS thread) and
//! `sessionStorage` (memory). Runtimes of one plugin share them.

use std::{
  cell::{Cell, RefCell},
  collections::{BTreeMap, HashMap},
  path::{Path, PathBuf},
  rc::{Rc, Weak},
  sync::{Arc, Mutex},
};

use rquickjs::{Coerced, Ctx, Exception, JsLifetime, Promise, Result, class::Trace};

use super::io::{Fail, Null, Res, blocking, promise};
use crate::{PluginRef, caps::CapabilityError};

pub(crate) struct Store {
  map: RefCell<BTreeMap<String, String>>,
  /// Where it persists; `None` for session storage
  file: Option<PathBuf>,
  revision: Cell<u64>,
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
      map: RefCell::new(map),
      file,
      revision: Cell::new(0),
      written: Arc::default(),
    })
  }

  /// The store persisted in `file`, shared while any runtime holds it
  fn file(file: PathBuf) -> Rc<Self> {
    FILES.with_borrow_mut(|files| {
      if let Some(store) = files.get(&file).and_then(Weak::upgrade) {
        return store;
      }
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
    let json = serde_json::to_vec_pretty(&*self.map.borrow()).expect("strings");
    let written = self.written.clone();
    Some(blocking(move || {
      let mut on_disk = written.lock().unwrap();
      if *on_disk >= revision {
        return Ok(());
      }
      write(&file, &json)
        .map_err(|e| Fail::Error(format!("writing {} failed: {e}", file.display())))?;
      *on_disk = revision;
      Ok(())
    }))
  }

  /// After every change. A failed write parks until `flush()` retries it.
  fn changed(&self) {
    self.revision.set(self.revision.get() + 1);
    if let Some(write) = self.persist() {
      super::io::detach(async move {
        if let Err(Fail::Error(e)) = write.await {
          tracing::warn!("storage not saved: {e}");
        }
      });
    }
  }
}

/// Atomically: a temporary file renamed over the target
fn write(file: &Path, json: &[u8]) -> std::io::Result<()> {
  if let Some(dir) = file.parent() {
    std::fs::create_dir_all(dir)?;
  }
  let tmp = file.with_extension("json.tmp");
  std::fs::write(&tmp, json)?;
  std::fs::rename(&tmp, file).inspect_err(|_| {
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
    self.map(&ctx)?.borrow_mut().insert(key.0, value.0);
    self.store.changed();
    Ok(())
  }

  #[qjs(rename = "removeItem")]
  pub fn remove_item(&self, ctx: Ctx<'_>, key: Coerced<String>) -> Result<()> {
    if self.map(&ctx)?.borrow_mut().remove(&key.0).is_some() {
      self.store.changed();
    }
    Ok(())
  }

  pub fn clear(&self, ctx: Ctx<'_>) -> Result<()> {
    self.map(&ctx)?.borrow_mut().clear();
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
