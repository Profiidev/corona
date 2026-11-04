//! The modules without authority: `buffer`, `path`, `url`, `crypto`, `zlib`, `console`.

use std::io::Read;

use base64::{
  Engine,
  engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE_NO_PAD},
};
use flate2::{
  Compression,
  read::{DeflateDecoder, GzDecoder, ZlibDecoder},
  write::{GzEncoder, ZlibEncoder},
};
use rquickjs::{
  ArrayBuffer, Coerced, Ctx, Exception, Function, JsLifetime, Object, Result, Value,
  class::Trace,
  function::{Opt, Rest},
};
use sha2::Digest;

use super::io::{Buf, bytes};
use crate::runtime::plugin::plugin;

/// Defines a module exporting `names` of the object `make` builds, and it as `default`.
macro_rules! object_module {
  ($module:ident, [$($export:literal),* $(,)?], $make:path) => {
    pub struct $module;

    impl rquickjs::module::ModuleDef for $module {
      fn declare<'js>(decl: &rquickjs::module::Declarations<'js>) -> rquickjs::Result<()> {
        $(decl.declare($export)?;)*
        decl.declare("default")?;
        Ok(())
      }

      fn evaluate<'js>(
        ctx: &rquickjs::Ctx<'js>,
        exports: &rquickjs::module::Exports<'js>,
      ) -> rquickjs::Result<()> {
        let o: rquickjs::Object = $make(ctx)?;
        $(exports.export($export, o.get::<_, rquickjs::Value>($export)?)?;)*
        exports.export("default", o)?;
        Ok(())
      }
    }
  };
}

// --- buffer

fn hex(bytes: &[u8]) -> String {
  bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn encode(ctx: Ctx<'_>, text: String, encoding: Opt<String>) -> Result<ArrayBuffer<'_>> {
  let encoding = encoding
    .0
    .unwrap_or_else(|| "utf8".into())
    .to_ascii_lowercase();
  let invalid = |_| Exception::throw_type(&ctx, &format!("invalid {encoding} string"));
  let out = match encoding.as_str() {
    "utf8" | "utf-8" => text.into_bytes(),
    "hex" => (0..text.len() / 2)
      .map(|i| u8::from_str_radix(text.get(i * 2..i * 2 + 2).unwrap_or("zz"), 16))
      .collect::<std::result::Result<_, _>>()
      .map_err(|_| Exception::throw_type(&ctx, "invalid hex string"))?,
    "base64" => STANDARD_NO_PAD
      .decode(text.trim_end_matches('='))
      .map_err(invalid)?,
    "base64url" => URL_SAFE_NO_PAD
      .decode(text.trim_end_matches('='))
      .map_err(invalid)?,
    "latin1" | "binary" | "ascii" => text.chars().map(|c| c as u32 as u8).collect(),
    other => {
      return Err(Exception::throw_type(
        &ctx,
        &format!("unknown encoding `{other}`"),
      ));
    }
  };
  ArrayBuffer::new(ctx.clone(), out)
}

fn decode(ctx: Ctx<'_>, data: Value<'_>, encoding: Opt<String>) -> Result<String> {
  let data = bytes(&data, "buffer.toString()")?;
  Ok(
    match encoding
      .0
      .unwrap_or_else(|| "utf8".into())
      .to_ascii_lowercase()
      .as_str()
    {
      "utf8" | "utf-8" => String::from_utf8_lossy(&data).into_owned(),
      "hex" => hex(&data),
      "base64" => STANDARD.encode(&data),
      "base64url" => URL_SAFE_NO_PAD.encode(&data),
      "latin1" | "binary" | "ascii" => data.iter().map(|b| *b as char).collect(),
      other => {
        return Err(Exception::throw_type(
          &ctx,
          &format!("unknown encoding `{other}`"),
        ));
      }
    },
  )
}

const BUFFER: &str = r#"(encode, decode) => class Buffer extends Uint8Array {
  static from(value, encoding) {
    if (typeof value === "string") return new Buffer(encode(value, encoding));
    if (value instanceof ArrayBuffer) return new Buffer(value.slice(0));
    const out = new Buffer(value.length);
    out.set(value);
    return out;
  }
  static alloc(size) { return new Buffer(size); }
  static isBuffer(value) { return value instanceof Buffer; }
  static byteLength(value, encoding) { return Buffer.from(value, encoding).length; }
  static concat(list) {
    const out = new Buffer(list.reduce((n, b) => n + b.length, 0));
    let at = 0;
    for (const b of list) { out.set(b, at); at += b.length; }
    return out;
  }
  toString(encoding) { return decode(this, encoding); }
}"#;

/// The global `Buffer`
pub(crate) fn buffer_class<'js>(ctx: &Ctx<'js>) -> Result<Value<'js>> {
  let make: Function = ctx.eval(BUFFER)?;
  make.call((
    Function::new(ctx.clone(), encode)?,
    Function::new(ctx.clone(), decode)?,
  ))
}

fn buffer_object<'js>(ctx: &Ctx<'js>) -> Result<Object<'js>> {
  let o = Object::new(ctx.clone())?;
  o.set("Buffer", ctx.globals().get::<_, Value>("Buffer")?)?;
  Ok(o)
}

object_module!(BufferModule, ["Buffer"], buffer_object);

// --- path

fn normalize(path: &str) -> String {
  if path.is_empty() {
    return ".".into();
  }
  let absolute = path.starts_with('/');
  let mut out: Vec<&str> = Vec::new();
  for part in path.split('/') {
    match part {
      "" | "." => {}
      ".." if out.last().is_some_and(|p| *p != "..") => {
        out.pop();
      }
      ".." if absolute => {}
      part => out.push(part),
    }
  }
  let mut joined = out.join("/");
  if absolute {
    joined.insert(0, '/');
  }
  if joined.is_empty() {
    joined.push('.');
  }
  if path.ends_with('/') && !joined.ends_with('/') {
    joined.push('/');
  }
  joined
}

fn join(parts: Rest<String>) -> String {
  let parts: Vec<_> = parts.0.into_iter().filter(|p| !p.is_empty()).collect();
  normalize(&parts.join("/"))
}

/// Against `/`: a plugin has no working directory
fn resolve(parts: Rest<String>) -> String {
  let mut out = String::new();
  for part in parts.0.iter().rev().filter(|p| !p.is_empty()) {
    out = format!("{part}/{out}");
    if part.starts_with('/') {
      break;
    }
  }
  let out = normalize(&format!("/{out}"));
  match out.len() > 1 {
    true => out.trim_end_matches('/').to_string(),
    false => out,
  }
}

fn dirname(path: String) -> String {
  let trimmed = path.trim_end_matches('/');
  match trimmed.rfind('/') {
    None if path.starts_with('/') => "/".into(),
    None => ".".into(),
    Some(0) => "/".into(),
    Some(i) => trimmed[..i].to_string(),
  }
}

fn basename(path: String, suffix: Opt<String>) -> String {
  let trimmed = path.trim_end_matches('/');
  let base = trimmed.rsplit('/').next().unwrap_or_default();
  match suffix.0 {
    Some(s) if base != s && base.ends_with(&s) => base[..base.len() - s.len()].to_string(),
    _ => base.to_string(),
  }
}

fn extname(path: String) -> String {
  let base = basename(path, Opt(None));
  match base.rfind('.') {
    Some(i) if i > 0 => base[i..].to_string(),
    _ => String::new(),
  }
}

fn path_object<'js>(ctx: &Ctx<'js>) -> Result<Object<'js>> {
  let o = Object::new(ctx.clone())?;
  o.set("join", Function::new(ctx.clone(), join)?)?;
  o.set("resolve", Function::new(ctx.clone(), resolve)?)?;
  o.set(
    "normalize",
    Function::new(ctx.clone(), |p: String| normalize(&p))?,
  )?;
  o.set("dirname", Function::new(ctx.clone(), dirname)?)?;
  o.set("basename", Function::new(ctx.clone(), basename)?)?;
  o.set("extname", Function::new(ctx.clone(), extname)?)?;
  o.set(
    "isAbsolute",
    Function::new(ctx.clone(), |p: String| p.starts_with('/'))?,
  )?;
  o.set("sep", "/")?;
  Ok(o)
}

object_module!(
  PathModule,
  [
    "join",
    "resolve",
    "normalize",
    "dirname",
    "basename",
    "extname",
    "isAbsolute",
    "sep"
  ],
  path_object
);

// --- url

/// `URL`, read-only
#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "URL")]
pub struct JsUrl {
  #[qjs(skip_trace)]
  url: url::Url,
}

#[rquickjs::methods]
impl JsUrl {
  #[qjs(constructor)]
  pub fn new(ctx: Ctx<'_>, input: String, base: Opt<String>) -> Result<Self> {
    let parsed = match base.0 {
      Some(base) => url::Url::parse(&base).and_then(|b| b.join(&input)),
      None => url::Url::parse(&input),
    };
    let url =
      parsed.map_err(|e| Exception::throw_type(&ctx, &format!("invalid URL `{input}`: {e}")))?;
    Ok(Self { url })
  }

  #[qjs(get)]
  pub fn href(&self) -> String {
    self.url.to_string()
  }

  #[qjs(get)]
  pub fn protocol(&self) -> String {
    format!("{}:", self.url.scheme())
  }

  #[qjs(get)]
  pub fn host(&self) -> String {
    let host = self.url.host_str().unwrap_or_default();
    match self.url.port() {
      Some(port) => format!("{host}:{port}"),
      None => host.into(),
    }
  }

  #[qjs(get)]
  pub fn hostname(&self) -> String {
    self.url.host_str().unwrap_or_default().into()
  }

  #[qjs(get)]
  pub fn port(&self) -> String {
    self.url.port().map(|p| p.to_string()).unwrap_or_default()
  }

  #[qjs(get)]
  pub fn pathname(&self) -> String {
    self.url.path().into()
  }

  #[qjs(get)]
  pub fn search(&self) -> String {
    self
      .url
      .query()
      .map(|q| format!("?{q}"))
      .unwrap_or_default()
  }

  #[qjs(get)]
  pub fn hash(&self) -> String {
    self
      .url
      .fragment()
      .map(|f| format!("#{f}"))
      .unwrap_or_default()
  }

  #[qjs(get)]
  pub fn origin(&self) -> String {
    self.url.origin().ascii_serialization()
  }

  #[qjs(get)]
  pub fn username(&self) -> String {
    self.url.username().into()
  }

  #[qjs(get)]
  pub fn password(&self) -> String {
    self.url.password().unwrap_or_default().into()
  }

  #[qjs(rename = "toString")]
  pub fn to_string_js(&self) -> String {
    self.url.to_string()
  }

  #[qjs(rename = "toJSON")]
  pub fn to_json(&self) -> String {
    self.url.to_string()
  }
}

const SEARCH_PARAMS: &str = r#"(class URLSearchParams {
  #list = [];
  constructor(init) {
    if (typeof init === "string") {
      for (const pair of init.replace(/^\?/, "").split("&").filter(Boolean)) {
        const i = pair.indexOf("=");
        const d = (s) => {
          s = s.replace(/\+/g, " ");
          try { return decodeURIComponent(s); } catch { return s; }
        };
        this.#list.push(i < 0 ? [d(pair), ""] : [d(pair.slice(0, i)), d(pair.slice(i + 1))]);
      }
    } else if (init) {
      for (const [k, v] of Array.isArray(init) ? init : Object.entries(init)) this.#list.push([String(k), String(v)]);
    }
  }
  append(k, v) { this.#list.push([String(k), String(v)]); }
  delete(k) { this.#list = this.#list.filter(([n]) => n !== k); }
  get(k) { return this.#list.find(([n]) => n === k)?.[1] ?? null; }
  getAll(k) { return this.#list.filter(([n]) => n === k).map(([, v]) => v); }
  has(k) { return this.#list.some(([n]) => n === k); }
  set(k, v) { this.delete(k); this.append(k, v); }
  forEach(f) { for (const [k, v] of this.#list) f(v, k, this); }
  entries() { return this.#list[Symbol.iterator](); }
  [Symbol.iterator]() { return this.entries(); }
  toString() { return this.#list.map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`.replace(/%20/g, "+")).join("&"); }
})"#;

/// Installs the global `URL` and `URLSearchParams`
pub(crate) fn install_url(ctx: &Ctx<'_>) -> Result<()> {
  rquickjs::Class::<JsUrl>::define(&ctx.globals())?;
  let params: Value = ctx.eval(SEARCH_PARAMS)?;
  ctx.globals().set("URLSearchParams", params)
}

fn url_object<'js>(ctx: &Ctx<'js>) -> Result<Object<'js>> {
  let o = Object::new(ctx.clone())?;
  o.set("URL", ctx.globals().get::<_, Value>("URL")?)?;
  o.set(
    "URLSearchParams",
    ctx.globals().get::<_, Value>("URLSearchParams")?,
  )?;
  Ok(o)
}

object_module!(UrlModule, ["URL", "URLSearchParams"], url_object);

// --- crypto

enum Hasher {
  Md5(md5::Md5),
  Sha1(sha1::Sha1),
  Sha256(sha2::Sha256),
  Sha512(sha2::Sha512),
}

/// `createHash(algorithm)`
#[derive(Trace, JsLifetime)]
#[rquickjs::class]
pub struct Hash {
  /// `None` once digested
  #[qjs(skip_trace)]
  hasher: Option<Hasher>,
}

#[rquickjs::methods]
impl Hash {
  pub fn update<'js>(
    this: rquickjs::function::This<rquickjs::Class<'js, Self>>,
    ctx: Ctx<'js>,
    data: Value<'js>,
  ) -> Result<rquickjs::Class<'js, Self>> {
    let data = bytes(&data, "hash.update(data)")?;
    match this.0.borrow_mut().hasher.as_mut() {
      Some(Hasher::Md5(h)) => h.update(&data),
      Some(Hasher::Sha1(h)) => h.update(&data),
      Some(Hasher::Sha256(h)) => h.update(&data),
      Some(Hasher::Sha512(h)) => h.update(&data),
      None => {
        return Err(Exception::throw_message(
          &ctx,
          "the hash is already digested",
        ));
      }
    }
    Ok(this.0.clone())
  }

  pub fn digest<'js>(&mut self, ctx: Ctx<'js>, encoding: Opt<String>) -> Result<Value<'js>> {
    let out = match self.hasher.take() {
      Some(Hasher::Md5(h)) => h.finalize().to_vec(),
      Some(Hasher::Sha1(h)) => h.finalize().to_vec(),
      Some(Hasher::Sha256(h)) => h.finalize().to_vec(),
      Some(Hasher::Sha512(h)) => h.finalize().to_vec(),
      None => {
        return Err(Exception::throw_message(
          &ctx,
          "the hash is already digested",
        ));
      }
    };
    use rquickjs::IntoJs;
    match encoding.0.as_deref() {
      None | Some("buffer") => Buf(out).into_js(&ctx),
      Some("hex") => hex(&out).into_js(&ctx),
      Some("base64") => STANDARD.encode(&out).into_js(&ctx),
      Some("base64url") => URL_SAFE_NO_PAD.encode(&out).into_js(&ctx),
      Some(other) => Err(Exception::throw_type(
        &ctx,
        &format!("unknown encoding `{other}`"),
      )),
    }
  }
}

fn create_hash(ctx: Ctx<'_>, algorithm: String) -> Result<Hash> {
  let hasher = match algorithm.to_ascii_lowercase().as_str() {
    "md5" => Hasher::Md5(Default::default()),
    "sha1" => Hasher::Sha1(Default::default()),
    "sha256" => Hasher::Sha256(Default::default()),
    "sha512" => Hasher::Sha512(Default::default()),
    other => {
      return Err(Exception::throw_type(
        &ctx,
        &format!("unknown hash `{other}`"),
      ));
    }
  };
  Ok(Hash {
    hasher: Some(hasher),
  })
}

fn random_bytes(ctx: Ctx<'_>, size: u32) -> Result<Buf> {
  if size > 65536 {
    return Err(Exception::throw_range(
      &ctx,
      "randomBytes(size) takes at most 65536",
    ));
  }
  let mut out = vec![0; size as usize];
  getrandom::fill(&mut out).map_err(|e| Exception::throw_message(&ctx, &e.to_string()))?;
  Ok(Buf(out))
}

fn crypto_object<'js>(ctx: &Ctx<'js>) -> Result<Object<'js>> {
  let o = Object::new(ctx.clone())?;
  o.set("createHash", Function::new(ctx.clone(), create_hash)?)?;
  o.set("randomBytes", Function::new(ctx.clone(), random_bytes)?)?;
  o.set(
    "randomUUID",
    Function::new(ctx.clone(), || uuid::Uuid::new_v4().to_string())?,
  )?;
  Ok(o)
}

object_module!(
  CryptoModule,
  ["createHash", "randomBytes", "randomUUID"],
  crypto_object
);

// --- zlib

const MAX_INFLATED: u64 = 64 * 1024 * 1024;

fn inflate(ctx: &Ctx<'_>, reader: impl Read) -> Result<Buf> {
  let mut out = Vec::new();
  reader
    .take(MAX_INFLATED + 1)
    .read_to_end(&mut out)
    .map_err(|e| Exception::throw_message(ctx, &format!("inflating failed: {e}")))?;
  if out.len() as u64 > MAX_INFLATED {
    return Err(Exception::throw_range(
      ctx,
      &format!("inflated data exceeded the {MAX_INFLATED} byte limit"),
    ));
  }
  Ok(Buf(out))
}

fn deflate<W: std::io::Write>(
  ctx: &Ctx<'_>,
  mut encoder: W,
  data: &[u8],
  finish: impl FnOnce(W) -> std::io::Result<Vec<u8>>,
) -> Result<Buf> {
  let fail = |e: std::io::Error| Exception::throw_message(ctx, &format!("deflating failed: {e}"));
  encoder.write_all(data).map_err(fail)?;
  finish(encoder).map(Buf).map_err(fail)
}

fn zlib_object<'js>(ctx: &Ctx<'js>) -> Result<Object<'js>> {
  let o = Object::new(ctx.clone())?;
  o.set(
    "deflateSync",
    Function::new(ctx.clone(), |ctx: Ctx<'js>, data: Value<'js>| {
      let data = bytes(&data, "deflateSync(data)")?;
      deflate(
        &ctx,
        ZlibEncoder::new(Vec::new(), Compression::default()),
        &data,
        |e| e.finish(),
      )
    })?,
  )?;
  o.set(
    "gzipSync",
    Function::new(ctx.clone(), |ctx: Ctx<'js>, data: Value<'js>| {
      let data = bytes(&data, "gzipSync(data)")?;
      deflate(
        &ctx,
        GzEncoder::new(Vec::new(), Compression::default()),
        &data,
        |e| e.finish(),
      )
    })?,
  )?;
  o.set(
    "inflateSync",
    Function::new(ctx.clone(), |ctx: Ctx<'js>, data: Value<'js>| {
      let data = bytes(&data, "inflateSync(data)")?;
      inflate(&ctx, ZlibDecoder::new(&data[..]))
    })?,
  )?;
  o.set(
    "inflateRawSync",
    Function::new(ctx.clone(), |ctx: Ctx<'js>, data: Value<'js>| {
      let data = bytes(&data, "inflateRawSync(data)")?;
      inflate(&ctx, DeflateDecoder::new(&data[..]))
    })?,
  )?;
  o.set(
    "gunzipSync",
    Function::new(ctx.clone(), |ctx: Ctx<'js>, data: Value<'js>| {
      let data = bytes(&data, "gunzipSync(data)")?;
      inflate(&ctx, GzDecoder::new(&data[..]))
    })?,
  )?;
  Ok(o)
}

object_module!(
  ZlibModule,
  [
    "deflateSync",
    "inflateSync",
    "inflateRawSync",
    "gzipSync",
    "gunzipSync"
  ],
  zlib_object
);

// --- console

fn text(value: &Value<'_>) -> String {
  if let Some(s) = value.as_string() {
    return s.to_string().unwrap_or_default();
  }
  if let Some(e) = value.as_exception() {
    return e.to_string();
  }
  if value.is_object()
    && !value.is_function()
    && let Ok(Some(json)) = value.ctx().json_stringify(value.clone())
  {
    return json.to_string().unwrap_or_default();
  }
  value
    .get::<Coerced<String>>()
    .map(|s| s.0)
    .unwrap_or_default()
}

fn log(level: tracing::Level) -> impl for<'js> Fn(Ctx<'js>, Rest<Value<'js>>) {
  move |ctx, values| {
    let plugin = plugin(&ctx).map(|p| p.id.clone()).unwrap_or_default();
    let line = values.0.iter().map(text).collect::<Vec<_>>().join(" ");
    match level {
      tracing::Level::ERROR => tracing::error!(target: "corona_js::script", %plugin, "{line}"),
      tracing::Level::WARN => tracing::warn!(target: "corona_js::script", %plugin, "{line}"),
      tracing::Level::INFO => tracing::info!(target: "corona_js::script", %plugin, "{line}"),
      _ => tracing::debug!(target: "corona_js::script", %plugin, "{line}"),
    }
  }
}

/// The `console` object, also a global
pub(crate) fn console_object<'js>(ctx: &Ctx<'js>) -> Result<Object<'js>> {
  use tracing::Level;
  let o = Object::new(ctx.clone())?;
  for (name, level) in [
    ("debug", Level::DEBUG),
    ("log", Level::INFO),
    ("info", Level::INFO),
    ("warn", Level::WARN),
    ("error", Level::ERROR),
  ] {
    o.set(name, Function::new(ctx.clone(), log(level))?)?;
  }
  Ok(o)
}

fn console_global<'js>(ctx: &Ctx<'js>) -> Result<Object<'js>> {
  let o = Object::new(ctx.clone())?;
  o.set("console", ctx.globals().get::<_, Value>("console")?)?;
  Ok(o)
}

pub struct ConsoleModule;

impl rquickjs::module::ModuleDef for ConsoleModule {
  fn declare<'js>(decl: &rquickjs::module::Declarations<'js>) -> Result<()> {
    decl.declare("default")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &rquickjs::module::Exports<'js>) -> Result<()> {
    let o = console_global(ctx)?;
    exports.export("default", o.get::<_, Value>("console")?)?;
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn paths_normalize_like_posix() {
    assert_eq!(normalize("a/./b/../c/"), "a/c/");
    assert_eq!(normalize("/../a"), "/a");
    assert_eq!(normalize("../a"), "../a");
    assert_eq!(join(Rest(vec!["a".into(), "".into(), "../b".into()])), "b");
    assert_eq!(
      resolve(Rest(vec!["/x".into(), "y/".into(), "../z".into()])),
      "/x/z"
    );
    assert_eq!(dirname("/a/b/".into()), "/a");
    assert_eq!(dirname("a".into()), ".");
    assert_eq!(basename("/a/b.txt".into(), Opt(Some(".txt".into()))), "b");
    assert_eq!(extname("a/.hidden".into()), "");
  }
}
