//! `fetch`: HTTP to granted hosts and paths, every redirect authorized again, or HTTP/1.1 to
//! a granted unix socket. Not the browser's: a response is a plain object.

use std::{io::Cursor, pin::Pin, rc::Rc, sync::Arc, time::Duration};

use futures::{Stream, StreamExt};
use http_body_util::{BodyStream, Full};
use hyper_util::rt::TokioIo;
use reqwest::{Method, StatusCode, Url, header::HeaderMap};
use rquickjs::{
  ArrayBuffer, CatchResultExt, Class, Ctx, Exception, Function, IntoJs, JsLifetime, Object,
  Promise, Result, Value, class::Trace, function::Opt,
};
use tokio::{
  net::UnixStream,
  sync::{Mutex, watch},
  time::timeout,
};

use super::{
  io::{Bytes, Fail, Null, Res, bytes, detach, io, promise},
  net::reachable,
};
use crate::{Capabilities, plugin::plugin};

const MAX_BODY: usize = 8 * 1024 * 1024;
const MAX_REDIRECTS: usize = 10;
const TIMEOUT_MS: f64 = 30_000.;
const MAX_TIMEOUT_MS: f64 = 300_000.;

type Body = Pin<Box<dyn Stream<Item = std::result::Result<Vec<u8>, String>> + Send>>;

struct Options {
  method: Method,
  headers: HeaderMap,
  body: Vec<u8>,
  timeout: Duration,
  stream: bool,
  unix: Option<String>,
}

fn prohibited(name: &str) -> bool {
  matches!(
    name.to_ascii_lowercase().as_str(),
    "host"
      | "content-length"
      | "connection"
      | "expect"
      | "proxy-authenticate"
      | "proxy-authorization"
      | "te"
      | "trailer"
      | "transfer-encoding"
      | "upgrade"
      | "origin"
  )
}

/// Request headers from a plain object, refusing the ones the client manages
pub(crate) fn headers<'js>(
  ctx: &Ctx<'js>,
  value: Option<Value<'js>>,
  what: &str,
  refused: fn(&str) -> bool,
) -> Result<HeaderMap> {
  let mut out = HeaderMap::new();
  let Some(object) = value.filter(|v| !v.is_undefined() && !v.is_null()) else {
    return Ok(out);
  };
  let Some(object) = object.into_object() else {
    return Err(Exception::throw_type(
      ctx,
      &format!("{what}.headers expects an object"),
    ));
  };
  for entry in object.props::<String, Value>() {
    let (name, value) = entry?;
    if refused(&name) {
      return Err(Exception::throw_type(
        ctx,
        &format!("{what}.headers may not set `{name}`"),
      ));
    }
    let value = value
      .as_string()
      .ok_or_else(|| Exception::throw_type(ctx, &format!("{what}.headers expects string values")))?
      .to_string()?;
    let invalid = || {
      Exception::throw_type(
        ctx,
        &format!("{what}.headers has an invalid header `{name}`"),
      )
    };
    let header = reqwest::header::HeaderName::from_bytes(name.as_bytes()).map_err(|_| invalid())?;
    let value = reqwest::header::HeaderValue::from_str(&value).map_err(|_| invalid())?;
    out.append(header, value);
  }
  Ok(out)
}

fn options<'js>(ctx: &Ctx<'js>, options: Option<Object<'js>>) -> Result<Options> {
  let mut out = Options {
    method: Method::GET,
    headers: HeaderMap::new(),
    body: Vec::new(),
    timeout: Duration::from_millis(TIMEOUT_MS as u64),
    stream: false,
    unix: None,
  };
  let Some(o) = options else {
    return Ok(out);
  };
  for key in o.keys::<String>() {
    let key = key?;
    let value: Value = o.get(&key)?;
    if value.is_undefined() {
      continue;
    }
    match key.as_str() {
      "method" => {
        let method: String = value.get()?;
        out.method = Method::from_bytes(method.to_ascii_uppercase().as_bytes()).map_err(|_| {
          Exception::throw_type(
            ctx,
            &format!("fetch(url, options).method `{method}` is not an HTTP method"),
          )
        })?;
      }
      "headers" => out.headers = headers(ctx, Some(value), "fetch(url, options)", prohibited)?,
      "body" => {
        out.body = bytes(&value, "fetch(url, options).body")?;
        if out.body.len() > MAX_BODY {
          return Err(Exception::throw_range(
            ctx,
            &format!("fetch request body exceeded the {MAX_BODY} byte limit"),
          ));
        }
      }
      "timeout" => {
        let ms = value.as_number().unwrap_or(f64::NAN);
        if !(1.0..=MAX_TIMEOUT_MS).contains(&ms) {
          return Err(Exception::throw_range(
            ctx,
            &format!("fetch(url, options).timeout must be 1 to {MAX_TIMEOUT_MS} milliseconds"),
          ));
        }
        out.timeout = Duration::from_millis(ms as u64);
      }
      "stream" => out.stream = value.get()?,
      "unix" => out.unix = Some(value.get()?),
      other => {
        return Err(Exception::throw_type(
          ctx,
          &format!("unknown fetch option `{other}`"),
        ));
      }
    }
  }
  Ok(out)
}

fn authorize(caps: &Capabilities, method: &Method, url: &Url) -> Res<()> {
  let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
  match caps.may_request(url.scheme(), &host, url.port(), method.as_str(), url.path()) {
    true => Ok(()),
    false => Err(Fail::Type(format!(
      "HTTP request {method} {url} is not granted; add it to capabilities.network.hosts or capabilities.network.http"
    ))),
  }
}

fn same_origin(a: &Url, b: &Url) -> bool {
  a.scheme() == b.scheme()
    && a.host_str().map(str::to_ascii_lowercase) == b.host_str().map(str::to_ascii_lowercase)
    && a.port_or_known_default() == b.port_or_known_default()
}

/// A redirect is a new request: granted on its own, and no credentials, body or headers
/// carried to another origin
fn authorize_redirect(
  caps: &Capabilities,
  method: &Method,
  from: &Url,
  to: &Url,
  headers: &HeaderMap,
) -> Res<()> {
  authorize(caps, method, to).map_err(|e| match e {
    Fail::Type(m) => Fail::Type(format!("redirect target refused: {m}")),
    other => other,
  })?;
  let refused = |why: String| {
    Err(Fail::Error(format!(
      "redirect from {from} to {to} refused because {why}"
    )))
  };
  if from.scheme() == "https" && to.scheme() != "https" {
    return refused("it is an HTTPS downgrade".into());
  }
  if same_origin(from, to) {
    return Ok(());
  }
  if *method != Method::GET {
    return refused(format!("it would replay a {method} request across origins"));
  }
  if !headers.is_empty() {
    return refused("caller-supplied request headers would be replayed across origins".into());
  }
  Ok(())
}

/// System DNS, filtered by [`reachable`], so a granted name is not rebound to a private
/// address. IP literals never reach it.
struct PublicResolver(Vec<String>);

impl reqwest::dns::Resolve for PublicResolver {
  fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
    let lan = self.0.iter().any(|h| h.eq_ignore_ascii_case(name.as_str()));
    Box::pin(async move {
      let host = name.as_str();
      let addrs = tokio::net::lookup_host((host, 0)).await?;
      let addrs = reachable(host, lan, addrs.collect()).map_err(|e| format!("{e:?}"))?;
      Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs)
    })
  }
}

/// What came back, the body unread
struct Head {
  status: u16,
  url: String,
  headers: Vec<(String, String)>,
}

fn head_headers<'a>(headers: impl Iterator<Item = (&'a str, &'a [u8])>) -> Vec<(String, String)> {
  let mut out: Vec<(String, String)> = Vec::new();
  for (name, value) in headers {
    let value = String::from_utf8_lossy(value).into_owned();
    match out.iter_mut().find(|(n, _)| n == name) {
      Some((_, v)) => {
        v.push_str(", ");
        v.push_str(&value);
      }
      None => out.push((name.to_ascii_lowercase(), value)),
    }
  }
  out
}

async fn send(caps: Capabilities, mut url: Url, o: Options) -> Res<(Head, Body)> {
  let lan_hosts = caps.lan_hosts().to_vec();
  // ponytail: a client per request, which loads the system roots each time; keep one
  // per LAN host set if fetch gets hot
  let client = reqwest::Client::builder()
    .connect_timeout(o.timeout)
    .no_proxy()
    .redirect_policy(reqwest::redirect::Policy::none())
    .dns_resolver(Arc::new(PublicResolver(lan_hosts)))
    .build()
    .map_err(|e| Fail::Error(format!("creating the HTTP client failed: {e}")))?;
  let (mut method, mut headers, mut body) = (o.method, o.headers, o.body);
  for _ in 0..=MAX_REDIRECTS {
    let request = client
      .request(method.clone(), url.clone())
      .headers(headers.clone())
      .body(body.clone())
      .send();
    let response = timeout(o.timeout, request)
      .await
      .map_err(|_| Fail::Error(format!("fetching {url} timed out")))?
      .map_err(|e| Fail::Error(format!("fetching {url} failed: {e}")))?;
    let status = response.status();
    let location = response.headers().get(reqwest::header::LOCATION);
    if let (true, Some(location)) = (redirects(status), location) {
      let next = location
        .to_str()
        .ok()
        .and_then(|l| url.join(l).ok())
        .ok_or_else(|| Fail::Error(format!("redirect from {url} has an invalid Location")))?;
      let get = (matches!(status, StatusCode::MOVED_PERMANENTLY | StatusCode::FOUND)
        && method == Method::POST)
        || (status == StatusCode::SEE_OTHER && method != Method::HEAD);
      if get {
        method = Method::GET;
        body.clear();
        headers.remove(reqwest::header::CONTENT_TYPE);
      }
      authorize_redirect(&caps, &method, &url, &next, &headers)?;
      url = next;
      continue;
    }
    let head = Head {
      status: status.as_u16(),
      url: url.to_string(),
      headers: head_headers(
        response
          .headers()
          .iter()
          .map(|(n, v)| (n.as_str(), v.as_bytes())),
      ),
    };
    let body = response
      .bytes_stream()
      .map(|chunk| chunk.map(|b| b.to_vec()).map_err(|e| e.to_string()));
    return Ok((head, Box::pin(body)));
  }
  Err(Fail::Error(format!(
    "fetch exceeded the {MAX_REDIRECTS} redirect limit"
  )))
}

fn redirects(status: StatusCode) -> bool {
  matches!(
    status,
    StatusCode::MOVED_PERMANENTLY
      | StatusCode::FOUND
      | StatusCode::SEE_OTHER
      | StatusCode::TEMPORARY_REDIRECT
      | StatusCode::PERMANENT_REDIRECT
  )
}

/// HTTP/1.1 over a unix socket; the URL's host only fills `Host`, redirects are returned
async fn send_unix(path: std::path::PathBuf, url: Url, o: Options) -> Res<(Head, Body)> {
  let failed = |e: String| {
    Fail::Error(format!(
      "fetching {url} over {} failed: {e}",
      path.display()
    ))
  };
  let exchange = async {
    let stream = UnixStream::connect(&path)
      .await
      .map_err(|e| failed(e.to_string()))?;
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
      .await
      .map_err(|e| failed(e.to_string()))?;
    tokio::spawn(conn);
    let target = match url.query() {
      Some(query) => format!("{}?{query}", url.path()),
      None => url.path().to_string(),
    };
    let host = match (url.host_str(), url.port()) {
      (Some(host), Some(port)) => format!("{host}:{port}"),
      (host, _) => host.unwrap_or("localhost").to_string(),
    };
    let mut request = hyper::Request::builder()
      .method(o.method.as_str())
      .uri(target)
      .header("host", host);
    for (name, value) in &o.headers {
      request = request.header(name.as_str(), value.as_bytes());
    }
    let request = request
      .body(Full::new(Cursor::new(o.body)))
      .map_err(|e| failed(e.to_string()))?;
    sender
      .send_request(request)
      .await
      .map_err(|e| failed(e.to_string()))
  };
  let response = timeout(o.timeout, exchange)
    .await
    .map_err(|_| Fail::Error(format!("fetching {url} timed out")))??;
  let (parts, body) = response.into_parts();
  let head = Head {
    status: parts.status.as_u16(),
    url: url.to_string(),
    headers: head_headers(
      parts
        .headers
        .iter()
        .map(|(n, v)| (n.as_str(), v.as_bytes())),
    ),
  };
  let body = BodyStream::new(body).filter_map(|frame| async move {
    match frame {
      Ok(frame) => frame.into_data().ok().map(|data| Ok(data.to_vec())),
      Err(e) => Some(Err(e.to_string())),
    }
  });
  Ok((head, Box::pin(body)))
}

/// The whole body, within the limit and a timeout per chunk
async fn collect(head: &Head, mut body: Body, per_read: Duration) -> Res<Vec<u8>> {
  let mut out = Vec::new();
  loop {
    let chunk = timeout(per_read, body.next())
      .await
      .map_err(|_| Fail::Error(format!("reading the response from {} timed out", head.url)))?;
    match chunk {
      None => return Ok(out),
      Some(Err(e)) => {
        return Err(Fail::Error(format!(
          "reading the response from {} failed: {e}",
          head.url
        )));
      }
      Some(Ok(chunk)) => out.extend_from_slice(&chunk),
    }
    if out.len() > MAX_BODY {
      return Err(Fail::Error(format!(
        "response body from {} exceeded the {MAX_BODY} byte limit",
        head.url
      )));
    }
  }
}

fn set_head<'js>(o: &Object<'js>, head: Head) -> Result<()> {
  o.set("status", head.status)?;
  o.set("ok", (200..300).contains(&head.status))?;
  o.set("url", head.url)?;
  let headers = Object::new(o.ctx().clone())?;
  for (name, value) in head.headers {
    headers.set(name, value)?;
  }
  o.set("headers", headers)
}

/// A promise already settled with what `f` makes, rejected with what it throws
fn settled<'js>(ctx: &Ctx<'js>, f: impl FnOnce() -> Result<Value<'js>>) -> Result<Promise<'js>> {
  let (promise, resolve, reject) = ctx.promise()?;
  match f().catch(ctx) {
    Ok(v) => resolve.call::<_, ()>((v,))?,
    Err(e) => {
      let message = e.to_string();
      reject.call::<_, ()>((Exception::from_message(ctx.clone(), &message)?,))?
    }
  }
  Ok(promise)
}

/// A fully read response
struct Response(Head, Vec<u8>);

impl<'js> IntoJs<'js> for Response {
  fn into_js(self, ctx: &Ctx<'js>) -> Result<Value<'js>> {
    let o = Object::new(ctx.clone())?;
    set_head(&o, self.0)?;
    let body = Rc::new(self.1);
    let b = body.clone();
    o.set(
      "text",
      Function::new(ctx.clone(), move |ctx: Ctx<'js>| {
        settled(&ctx, || {
          String::from_utf8_lossy(&b).into_owned().into_js(&ctx)
        })
      })?,
    )?;
    let b = body.clone();
    o.set(
      "json",
      Function::new(ctx.clone(), move |ctx: Ctx<'js>| {
        settled(&ctx, || {
          ctx.json_parse(String::from_utf8_lossy(&b).into_owned())
        })
      })?,
    )?;
    o.set(
      "arrayBuffer",
      Function::new(ctx.clone(), move |ctx: Ctx<'js>| {
        settled(&ctx, || {
          ArrayBuffer::new(ctx.clone(), body.to_vec()).map(|b| b.into_value())
        })
      })?,
    )?;
    Ok(o.into_value())
  }
}

/// `fetch(url, { stream: true })`: the body read chunk by chunk, without a limit
#[derive(Trace, JsLifetime)]
#[rquickjs::class]
pub struct FetchStream {
  #[qjs(skip_trace)]
  body: Arc<Mutex<Option<Body>>>,
  #[qjs(skip_trace)]
  closed: watch::Sender<bool>,
}

#[rquickjs::methods]
impl FetchStream {
  /// The next chunk; null at the end or after close
  pub fn read<'js>(&self, ctx: Ctx<'js>) -> Result<Promise<'js>> {
    let (body, mut closed) = (self.body.clone(), self.closed.subscribe());
    promise(
      &ctx,
      io(async move {
        let read = async {
          let mut body = body.lock().await;
          let Some(stream) = body.as_mut() else {
            return Ok(Null(None));
          };
          match stream.next().await {
            Some(Ok(chunk)) => Ok(Null(Some(Bytes(chunk)))),
            Some(Err(e)) => Err(Fail::Error(format!("reading the response failed: {e}"))),
            None => {
              body.take();
              Ok(Null(None))
            }
          }
        };
        tokio::select! {
          biased;
          _ = closed.wait_for(|c| *c) => Ok(Null(None)),
          read = read => read,
        }
      }),
    )
  }

  /// Stops reading and releases the connection.
  pub fn close(&self) {
    let _ = self.closed.send(true);
    let body = self.body.clone();
    detach(async move {
      body.lock().await.take();
    });
  }
}

impl Drop for FetchStream {
  fn drop(&mut self) {
    let _ = self.closed.send(true);
  }
}

struct Streaming(Head, Body);

impl<'js> IntoJs<'js> for Streaming {
  fn into_js(self, ctx: &Ctx<'js>) -> Result<Value<'js>> {
    let stream = FetchStream {
      body: Arc::new(Mutex::new(Some(self.1))),
      closed: watch::channel(false).0,
    };
    let instance = Class::instance(ctx.clone(), stream)?;
    set_head(instance.as_object().expect("an instance"), self.0)?;
    Ok(instance.into_value())
  }
}

enum Fetched {
  Whole(Response),
  Stream(Streaming),
}

impl<'js> IntoJs<'js> for Fetched {
  fn into_js(self, ctx: &Ctx<'js>) -> Result<Value<'js>> {
    match self {
      Fetched::Whole(r) => r.into_js(ctx),
      Fetched::Stream(s) => s.into_js(ctx),
    }
  }
}

pub(crate) fn fetch<'js>(
  ctx: Ctx<'js>,
  url: String,
  opts: Opt<Object<'js>>,
) -> Result<Promise<'js>> {
  let url = Url::parse(&url)
    .map_err(|e| Exception::throw_type(&ctx, &format!("invalid fetch URL: {e}")))?;
  let mut o = options(&ctx, opts.0)?;
  let (stream, per_read) = (o.stream, o.timeout);
  let sent = match o.unix.take() {
    Some(path) => {
      let path = super::net::unix_path(&ctx, &path)?;
      futures::future::Either::Left(io(send_unix(path, url, o)))
    }
    None => {
      let caps = plugin(&ctx)?.capabilities.clone();
      authorize(&caps, &o.method, &url).map_err(|f| f.throw(&ctx))?;
      futures::future::Either::Right(io(send(caps, url, o)))
    }
  };
  promise(&ctx, async move {
    let (head, body) = sent.await?;
    Ok(match stream {
      true => Fetched::Stream(Streaming(head, body)),
      false => {
        let read = io(async move {
          let body = collect(&head, body, per_read).await?;
          Ok(Response(head, body))
        });
        Fetched::Whole(read.await?)
      }
    })
  })
}
