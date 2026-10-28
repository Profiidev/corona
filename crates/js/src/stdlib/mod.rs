//! The runtime stdlib, every privileged call checked against the plugin's
//! [`Capabilities`](crate::Capabilities).
//!
//! Written here rather than taken from `llrt_modules`: its newest release (0.8.1-beta) is
//! built on rquickjs 0.11, whose types do not mix with the 0.14 this crate uses, and its
//! `fs`, `net`, `child_process` and `fetch` carry ambient authority that would have to be
//! wrapped call by call anyway.

use rquickjs::{
  Ctx, Function, Object, Result,
  loader::{BuiltinResolver, ModuleLoader},
};

pub use storage::Storage;

mod basics;
mod fetch;
mod fs;
mod io;
mod net;
mod process;
mod storage;
pub(crate) mod timers;
mod ws;

pub(crate) const NAMES: &[&str] = &[
  "buffer",
  "console",
  "crypto",
  "fs/promises",
  "net",
  "os",
  "path",
  "process",
  "url",
  "websocket",
  "zlib",
];

pub(crate) fn resolver() -> BuiltinResolver {
  NAMES
    .iter()
    .fold(BuiltinResolver::default(), |r, name| r.with_module(*name))
}

pub(crate) fn loader() -> ModuleLoader {
  ModuleLoader::default()
    .with_module("buffer", basics::BufferModule)
    .with_module("console", basics::ConsoleModule)
    .with_module("crypto", basics::CryptoModule)
    .with_module("fs/promises", fs::FsModule)
    .with_module("net", net::NetModule)
    .with_module("os", process::OsModule)
    .with_module("path", basics::PathModule)
    .with_module("process", process::ProcessModule)
    .with_module("url", basics::UrlModule)
    .with_module("websocket", ws::WebSocketModule)
    .with_module("zlib", basics::ZlibModule)
}

/// The globals: `console`, `process`, `Buffer`, `URL`, `fetch`, timers and the storages
/// of `plugin`.
pub(crate) fn install(ctx: &Ctx<'_>, plugin: &crate::PluginRef) -> Result<()> {
  let globals = ctx.globals();
  globals.set("console", basics::console_object(ctx)?)?;
  globals.set("process", process::object(ctx)?)?;
  globals.set("Buffer", basics::buffer_class(ctx)?)?;
  basics::install_url(ctx)?;
  globals.set("fetch", Function::new(ctx.clone(), fetch::fetch)?)?;
  timers::install(ctx)?;
  globals.set("localStorage", Storage::local(plugin))?;
  globals.set("sessionStorage", Storage::session(plugin))?;
  // `window.localStorage`, as in a browser, also on the window API's `window`
  let window = match globals.get::<_, Option<Object>>("window")? {
    Some(window) => window,
    None => Object::new(ctx.clone())?,
  };
  window.set(
    "localStorage",
    globals.get::<_, rquickjs::Value>("localStorage")?,
  )?;
  window.set(
    "sessionStorage",
    globals.get::<_, rquickjs::Value>("sessionStorage")?,
  )?;
  globals.set("window", window)
}

/// Typings of the stdlib
pub const D_TS: &str = include_str!("stdlib.d.ts");
