//! `websocket`: client sockets to granted hosts. Redirects are not followed.

use std::sync::Arc;

use futures::{
  SinkExt, StreamExt,
  stream::{SplitSink, SplitStream},
};
use rquickjs::{
  Ctx, Exception, Function, IntoJs, JsLifetime, Object, Promise, Result, Value,
  class::Trace,
  function::Opt,
  module::{Declarations, Exports, ModuleDef},
};
use tokio::{net::TcpStream, sync::Mutex, time::timeout};
use tokio_tungstenite::{
  MaybeTlsStream, WebSocketStream, client_async_tls_with_config,
  tungstenite::{Message, client::IntoClientRequest},
};
use url::Url;

use super::{
  fetch::headers,
  io::{Bytes, Fail, Res, bytes, io, promise},
  net::{TIMEOUT, tcp},
};
use crate::{caps::effective_port, plugin::plugin};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

const MAX_WRITE: usize = 1024 * 1024;

fn refused(name: &str) -> bool {
  let name = name.to_ascii_lowercase();
  matches!(
    name.as_str(),
    "authorization" | "proxy-authorization" | "cookie" | "host" | "connection" | "upgrade"
  ) || name.starts_with("sec-websocket-")
}

enum Received {
  Text(String),
  Binary(Vec<u8>),
}

impl<'js> IntoJs<'js> for Received {
  fn into_js(self, ctx: &Ctx<'js>) -> Result<Value<'js>> {
    match self {
      Received::Text(s) => s.into_js(ctx),
      Received::Binary(b) => Bytes(b).into_js(ctx),
    }
  }
}

#[derive(Trace, JsLifetime)]
#[rquickjs::class]
pub struct WebSocketSocket {
  #[qjs(skip_trace)]
  reader: Arc<Mutex<SplitStream<Ws>>>,
  #[qjs(skip_trace)]
  writer: Arc<Mutex<SplitSink<Ws, Message>>>,
}

fn failed(what: &str) -> impl Fn(tokio_tungstenite::tungstenite::Error) -> Fail + '_ {
  move |e| Fail::Error(format!("{what} the WebSocket failed: {e}"))
}

#[rquickjs::methods]
impl WebSocketSocket {
  /// The next text or binary message
  pub fn read<'js>(&self, ctx: Ctx<'js>) -> Result<Promise<'js>> {
    let reader = self.reader.clone();
    promise(
      &ctx,
      io(async move {
        let mut reader = reader.lock().await;
        loop {
          match reader.next().await {
            None | Some(Ok(Message::Close(_))) => {
              return Err(Fail::Error("the WebSocket is closed".into()));
            }
            Some(Err(e)) => return Err(failed("reading")(e)),
            Some(Ok(Message::Text(text))) => return Ok(Received::Text(text.to_string())),
            Some(Ok(Message::Binary(data))) => return Ok(Received::Binary(data.to_vec())),
            Some(Ok(_)) => {}
          }
        }
      }),
    )
  }

  pub fn write<'js>(&self, ctx: Ctx<'js>, data: Value<'js>) -> Result<Promise<'js>> {
    let message = match data.as_string() {
      Some(s) => Message::text(s.to_string()?),
      None => Message::binary(bytes(&data, "socket.write(data)")?),
    };
    if message.len() > MAX_WRITE {
      return Err(Exception::throw_range(
        &ctx,
        &format!("socket.write(data) exceeded the {MAX_WRITE} byte limit"),
      ));
    }
    let writer = self.writer.clone();
    promise(
      &ctx,
      io(async move {
        let send = writer.lock().await.send(message).await;
        send.map_err(failed("writing"))
      }),
    )
  }

  pub fn close<'js>(&self, ctx: Ctx<'js>) -> Result<Promise<'js>> {
    let writer = self.writer.clone();
    promise(
      &ctx,
      io(async move {
        let close = writer.lock().await.close().await;
        close.map_err(failed("closing"))
      }),
    )
  }
}

async fn connect_ws(url: Url, lan: bool, headers: reqwest::header::HeaderMap) -> Res<Ws> {
  let host = url.host_str().unwrap_or_default().to_string();
  let port = effective_port(url.scheme(), url.port()).unwrap_or(80);
  let mut request = url
    .as_str()
    .into_client_request()
    .map_err(|e| Fail::Type(format!("invalid WebSocket URL: {e}")))?;
  request.headers_mut().extend(headers);
  let handshake = async {
    let stream = tcp(&host, port, lan).await?;
    let (ws, _) = client_async_tls_with_config(request, stream, None, None)
      .await
      .map_err(|e| Fail::Error(format!("connecting to {url} failed: {e}")))?;
    Ok(ws)
  };
  timeout(TIMEOUT, handshake)
    .await
    .map_err(|_| Fail::Error(format!("connecting to {url} timed out")))?
}

fn connect<'js>(ctx: Ctx<'js>, url: String, options: Opt<Object<'js>>) -> Result<Promise<'js>> {
  let url = Url::parse(&url)
    .map_err(|e| Exception::throw_type(&ctx, &format!("invalid WebSocket URL: {e}")))?;
  if !matches!(url.scheme(), "ws" | "wss") {
    return Err(Exception::throw_type(
      &ctx,
      "WebSocket.connect(url) expects a ws:// or wss:// URL",
    ));
  }
  let mut extra = None;
  if let Some(o) = options.0 {
    for key in o.keys::<String>() {
      let key = key?;
      if key != "headers" {
        return Err(Exception::throw_type(
          &ctx,
          &format!("unknown option `{key}` for WebSocket.connect(url, options); expected headers"),
        ));
      }
    }
    extra = o.get("headers")?;
  }
  let extra = headers(&ctx, extra, "WebSocket.connect(url, options)", refused)?;
  let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
  let caps = &plugin(&ctx)?.capabilities;
  if !caps.may_reach(&host) {
    return Err(Exception::throw_type(
      &ctx,
      &format!("network access to `{host}` is not granted; add it to capabilities.network.hosts"),
    ));
  }
  let ws = io(connect_ws(url, caps.may_reach_lan(&host), extra));
  promise(&ctx, async move {
    let (writer, reader) = ws.await?.split();
    Ok(WebSocketSocket {
      reader: Arc::new(Mutex::new(reader)),
      writer: Arc::new(Mutex::new(writer)),
    })
  })
}

pub struct WebSocketModule;

impl ModuleDef for WebSocketModule {
  fn declare<'js>(decl: &Declarations<'js>) -> Result<()> {
    decl.declare("WebSocket")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> Result<()> {
    let ws = Object::new(ctx.clone())?;
    ws.set("connect", Function::new(ctx.clone(), connect)?)?;
    exports.export("WebSocket", ws)?;
    Ok(())
  }
}
