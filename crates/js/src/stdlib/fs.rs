//! `fs/promises`: every call goes through a granted directory handle, off the JS thread.

use std::{
  io::{Read, Write},
  path::Path,
  sync::{Arc, Mutex},
};

use cap_std::fs::{File, OpenOptions};
use rquickjs::{
  Ctx, Function, IntoJs, JsLifetime, Object, Promise, Result, Value,
  class::Trace,
  function::{Func, Opt},
  module::{Declarations, Exports, ModuleDef},
};

use super::io::{Bytes, Fail, Null, Res, blocking, bytes, promise};
use crate::{
  caps::{Access, Grant},
  plugin::plugin,
};

const MAX_READ: u64 = 64 * 1024 * 1024;
const MAX_WRITE: usize = 8 * 1024 * 1024;
/// Per call of a file handle
const MAX_CHUNK: usize = 1024 * 1024;
const CHUNK: usize = 64 * 1024;

/// Opens `path` for `access` with the calling plugin's grant, on a blocking thread
fn grant(ctx: &Ctx<'_>, path: &str, access: Access) -> Result<impl FnOnce() -> Res<Grant> + use<>> {
  let caps = plugin(ctx)?.capabilities.clone();
  let path = path.to_string();
  Ok(move || Ok(caps.open(Path::new(&path), access)?))
}

fn failed(op: &str, path: &str) -> impl FnOnce(std::io::Error) -> Fail {
  move |e| Fail::Error(format!("{op} `{path}` failed: {e}"))
}

fn utf8(encoding: Option<Value<'_>>) -> Result<bool> {
  let Some(encoding) = encoding.filter(|e| !e.is_undefined() && !e.is_null()) else {
    return Ok(false);
  };
  let name: String = match encoding.as_object() {
    Some(o) if !encoding.is_string() => o.get::<_, Option<String>>("encoding")?.unwrap_or_default(),
    _ => encoding.get()?,
  };
  match name.to_ascii_lowercase().as_str() {
    "utf8" | "utf-8" => Ok(true),
    "" => Ok(false),
    other => Err(rquickjs::Exception::throw_type(
      encoding.ctx(),
      &format!("unsupported encoding `{other}`"),
    )),
  }
}

/// Text or bytes
enum Contents {
  Text(String),
  Bytes(Vec<u8>),
}

impl<'js> IntoJs<'js> for Contents {
  fn into_js(self, ctx: &Ctx<'js>) -> Result<Value<'js>> {
    match self {
      Contents::Text(s) => s.into_js(ctx),
      Contents::Bytes(b) => Bytes(b).into_js(ctx),
    }
  }
}

fn read_file<'js>(ctx: Ctx<'js>, path: String, encoding: Opt<Value<'js>>) -> Result<Promise<'js>> {
  let text = utf8(encoding.0)?;
  let grant = grant(&ctx, &path, Access::Read)?;
  promise(&ctx, async move {
    blocking(move || {
      let g = grant()?;
      let mut file = g.dir.open(&g.relative).map_err(failed("reading", &path))?;
      let mut out = Vec::new();
      (&mut file)
        .take(MAX_READ + 1)
        .read_to_end(&mut out)
        .map_err(failed("reading", &path))?;
      if out.len() as u64 > MAX_READ {
        return Err(Fail::Range(format!(
          "reading `{path}` exceeded the {MAX_READ} byte limit"
        )));
      }
      Ok(match text {
        true => Contents::Text(String::from_utf8_lossy(&out).into_owned()),
        false => Contents::Bytes(out),
      })
    })
    .await
  })
}

fn write_file<'js>(ctx: Ctx<'js>, path: String, contents: Value<'js>) -> Result<Promise<'js>> {
  let data = bytes(&contents, "fs.writeFile(path, contents)")?;
  if data.len() > MAX_WRITE {
    return Err(
      Fail::Range(format!(
        "fs.writeFile(path, contents) exceeded the {MAX_WRITE} byte write limit"
      ))
      .throw(&ctx),
    );
  }
  let grant = grant(&ctx, &path, Access::Write)?;
  promise(&ctx, async move {
    blocking(move || {
      let g = grant()?;
      g.dir
        .write(&g.relative, data)
        .map_err(failed("writing", &path))?;
      Ok(())
    })
    .await
  })
}

fn readdir<'js>(ctx: Ctx<'js>, path: String, options: Opt<Object<'js>>) -> Result<Promise<'js>> {
  let typed = match options.0 {
    Some(o) => o.get::<_, Option<bool>>("withFileTypes")?.unwrap_or(false),
    None => false,
  };
  let grant = grant(&ctx, &path, Access::Read)?;
  promise(&ctx, async move {
    let mut entries = blocking(move || {
      let g = grant()?;
      let list = g
        .dir
        .read_dir(&g.relative)
        .map_err(failed("listing", &path))?;
      Ok(
        list
          .filter_map(|e| e.ok())
          .map(|e| {
            let dir = e.file_type().is_ok_and(|t| t.is_dir());
            (e.file_name().to_string_lossy().into_owned(), dir)
          })
          .collect::<Vec<_>>(),
      )
    })
    .await?;
    entries.sort();
    Ok(Entries(entries, typed))
  })
}

struct Entries(Vec<(String, bool)>, bool);

impl<'js> IntoJs<'js> for Entries {
  fn into_js(self, ctx: &Ctx<'js>) -> Result<Value<'js>> {
    if !self.1 {
      return (self.0.into_iter().map(|(name, _)| name))
        .collect::<Vec<_>>()
        .into_js(ctx);
    }
    let list = rquickjs::Array::new(ctx.clone())?;
    for (i, (name, dir)) in self.0.into_iter().enumerate() {
      let entry = Object::new(ctx.clone())?;
      entry.set("name", name)?;
      entry.set("isDirectory", Func::from(move || dir))?;
      entry.set("isFile", Func::from(move || !dir))?;
      list.set(i, entry)?;
    }
    Ok(list.into_value())
  }
}

fn exists<'js>(ctx: Ctx<'js>, path: String) -> Result<Promise<'js>> {
  let grant = grant(&ctx, &path, Access::Read)?;
  promise(&ctx, async move {
    blocking(move || {
      let g = grant()?;
      Ok(g.dir.exists(&g.relative))
    })
    .await
  })
}

/// Runs `op` against the write grant of `path`
fn change<'js>(
  ctx: Ctx<'js>,
  path: String,
  what: &'static str,
  op: impl FnOnce(&Grant) -> std::io::Result<()> + Send + 'static,
) -> Result<Promise<'js>> {
  let grant = grant(&ctx, &path, Access::Write)?;
  promise(&ctx, async move {
    blocking(move || op(&grant()?).map_err(failed(what, &path))).await
  })
}

fn unlink<'js>(ctx: Ctx<'js>, path: String) -> Result<Promise<'js>> {
  change(ctx, path, "removing", |g| g.dir.remove_file(&g.relative))
}

fn rmdir<'js>(ctx: Ctx<'js>, path: String) -> Result<Promise<'js>> {
  change(ctx, path, "removing", |g| g.dir.remove_dir(&g.relative))
}

fn mkdir<'js>(ctx: Ctx<'js>, path: String, options: Opt<Object<'js>>) -> Result<Promise<'js>> {
  let recursive = match options.0 {
    Some(o) => o.get::<_, Option<bool>>("recursive")?.unwrap_or(false),
    None => false,
  };
  change(ctx, path, "making", move |g| match recursive {
    true => g.dir.create_dir_all(&g.relative),
    false => g.dir.create_dir(&g.relative),
  })
}

fn open<'js>(ctx: Ctx<'js>, path: String, flags: Opt<String>) -> Result<Promise<'js>> {
  let flags = flags.0.unwrap_or_else(|| "r".into());
  let (read, write, append, truncate) = match flags.as_str() {
    "r" => (true, false, false, false),
    "w" => (false, true, false, true),
    "a" => (false, true, true, false),
    "r+" => (true, true, false, false),
    other => {
      return Err(Fail::Type(format!("fs.open(path, flags) does not know `{other}`")).throw(&ctx));
    }
  };
  let check_read = read.then(|| grant(&ctx, &path, Access::Read)).transpose()?;
  let check_write = write
    .then(|| grant(&ctx, &path, Access::Write))
    .transpose()?;
  promise(&ctx, async move {
    let file = blocking(move || {
      let read_grant = check_read.map(|g| g()).transpose()?;
      let write_grant = check_write.map(|g| g()).transpose()?;
      let g = write_grant.or(read_grant).expect("a grant");
      let mut options = OpenOptions::new();
      options
        .read(read)
        .write(write && !append)
        .append(append)
        .truncate(truncate)
        .create(write && !read);
      g.dir
        .open_with(&g.relative, &options)
        .map_err(failed("opening", &path))
    })
    .await?;
    Ok(FileHandle {
      file: Arc::new(Mutex::new(Some(file))),
    })
  })
}

/// An open file, closed by `close()` or when its runtime goes away
#[derive(Trace, JsLifetime)]
#[rquickjs::class]
pub struct FileHandle {
  #[qjs(skip_trace)]
  file: Arc<Mutex<Option<File>>>,
}

const CLOSED: &str = "the file is closed";

#[rquickjs::methods]
impl FileHandle {
  /// Up to `max_bytes`, null at EOF
  pub fn read<'js>(&self, ctx: Ctx<'js>, max_bytes: Opt<f64>) -> Result<Promise<'js>> {
    let max = max_bytes.0.unwrap_or(CHUNK as f64);
    if !(1.0..=MAX_CHUNK as f64).contains(&max) {
      return Err(Fail::Range(format!("file.read(maxBytes) must be 1 to {MAX_CHUNK}")).throw(&ctx));
    }
    let file = self.file.clone();
    promise(&ctx, async move {
      blocking(move || {
        let mut file = file.lock().unwrap();
        let file = file.as_mut().ok_or_else(|| Fail::Error(CLOSED.into()))?;
        let mut buf = vec![0; max as usize];
        let n = file
          .read(&mut buf)
          .map_err(|e| Fail::Error(format!("reading failed: {e}")))?;
        buf.truncate(n);
        Ok(Null((n > 0).then_some(Bytes(buf))))
      })
      .await
    })
  }

  pub fn write<'js>(&self, ctx: Ctx<'js>, data: Value<'js>) -> Result<Promise<'js>> {
    let data = bytes(&data, "file.write(data)")?;
    if data.len() > MAX_CHUNK {
      return Err(
        Fail::Range(format!(
          "file.write(data) exceeded the {MAX_CHUNK} byte limit"
        ))
        .throw(&ctx),
      );
    }
    let file = self.file.clone();
    promise(&ctx, async move {
      blocking(move || {
        let mut file = file.lock().unwrap();
        let file = file.as_mut().ok_or_else(|| Fail::Error(CLOSED.into()))?;
        file
          .write_all(&data)
          .map_err(|e| Fail::Error(format!("writing failed: {e}")))
      })
      .await
    })
  }

  pub fn close<'js>(&self, ctx: Ctx<'js>) -> Result<Promise<'js>> {
    let file = self.file.clone();
    promise(&ctx, async move {
      blocking(move || {
        file.lock().unwrap().take();
        Ok(())
      })
      .await
    })
  }
}

pub struct FsModule;

impl ModuleDef for FsModule {
  fn declare<'js>(decl: &Declarations<'js>) -> Result<()> {
    for name in [
      "readFile",
      "writeFile",
      "readdir",
      "exists",
      "unlink",
      "rmdir",
      "mkdir",
      "open",
      "default",
    ] {
      decl.declare(name)?;
    }
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> Result<()> {
    let all = Object::new(ctx.clone())?;
    let fns: [(&str, Function); 8] = [
      ("readFile", Function::new(ctx.clone(), read_file)?),
      ("writeFile", Function::new(ctx.clone(), write_file)?),
      ("readdir", Function::new(ctx.clone(), readdir)?),
      ("exists", Function::new(ctx.clone(), exists)?),
      ("unlink", Function::new(ctx.clone(), unlink)?),
      ("rmdir", Function::new(ctx.clone(), rmdir)?),
      ("mkdir", Function::new(ctx.clone(), mkdir)?),
      ("open", Function::new(ctx.clone(), open)?),
    ];
    for (name, f) in fns {
      all.set(name, f.clone())?;
      exports.export(name, f)?;
    }
    exports.export("default", all)?;
    Ok(())
  }
}
