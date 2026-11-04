//! `net`: TCP to granted hosts and granted unix sockets. Host names reach only public
//! addresses unless they are LAN hosts; a literal IP reaches what it names.

use std::{
  net::{IpAddr, SocketAddr},
  path::Path,
  sync::Arc,
  time::Duration,
};

use rquickjs::{
  Ctx, Exception, Function, JsLifetime, Object, Promise, Result, Value,
  class::Trace,
  function::Opt,
  module::{Declarations, Exports, ModuleDef},
};
use tokio::{
  io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadHalf, WriteHalf},
  net::{TcpStream, UnixStream},
  sync::{Mutex, watch},
  time::timeout,
};

use super::io::{Bytes, Fail, Null, Res, bytes, io, promise};
use crate::runtime::plugin::plugin;

pub(crate) const TIMEOUT: Duration = Duration::from_secs(30);
const MAX_CHUNK: usize = 1024 * 1024;
const CHUNK: usize = 64 * 1024;

/// Whether `ip` is a public unicast address
pub(crate) fn is_public(ip: IpAddr) -> bool {
  match ip {
    IpAddr::V4(ip) => {
      let [a, b, c, _] = ip.octets();
      !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_multicast()
        || a == 0
        || a >= 240
        || (a == 100 && b & 0xc0 == 64)
        || (a == 198 && b & 0xfe == 18)
        || matches!((a, b, c), (192, 0, 0 | 2) | (198, 51, 100) | (203, 0, 113)))
    }
    IpAddr::V6(ip) => {
      let s = ip.segments();
      let v4 =
        |hi: u16, lo: u16| is_public(IpAddr::V4(((u32::from(hi) << 16) | u32::from(lo)).into()));
      match s {
        [0, 0, 0, 0, 0, 0xffff | 0, ..] | [0x64, 0xff9b, 0, 0, 0, 0, ..] => v4(s[6], s[7]),
        [0x2002, ..] => v4(s[1], s[2]),
        _ => {
          !(ip.is_unspecified()
            || ip.is_loopback()
            || ip.is_multicast()
            || s[0] & 0xfe00 == 0xfc00
            || s[0] & 0xffc0 == 0xfe80
            || s[0] & 0xffc0 == 0xfec0
            || (s[0], s[1], s[2]) == (0x64, 0xff9b, 1)
            || (s[0], s[1]) == (0x2001, 0)
            || (s[0], s[1]) == (0x2001, 0xdb8)
            || s[..4] == [0x100, 0, 0, 0])
        }
      }
    }
  }
}

/// Private networks, not loopback or link-local
pub(crate) fn is_lan(ip: IpAddr) -> bool {
  match ip {
    IpAddr::V4(ip) => {
      let [a, b, ..] = ip.octets();
      ip.is_private() || (a == 100 && b & 0xc0 == 64)
    }
    IpAddr::V6(ip) => match ip.to_ipv4_mapped() {
      Some(ip) => is_lan(IpAddr::V4(ip)),
      None => ip.segments()[0] & 0xfe00 == 0xfc00,
    },
  }
}

/// The addresses `host` resolved to that it may reach
pub(crate) fn reachable(host: &str, lan: bool, addrs: Vec<SocketAddr>) -> Res<Vec<SocketAddr>> {
  let literal = host
    .trim_start_matches('[')
    .trim_end_matches(']')
    .parse::<IpAddr>()
    .is_ok();
  let localhost = host.eq_ignore_ascii_case("localhost");
  let resolved = addrs.len();
  let allowed: Vec<_> = addrs
    .into_iter()
    .filter(|a| {
      literal
        || match localhost {
          true => a.ip().is_loopback(),
          false => is_public(a.ip()) || (lan && is_lan(a.ip())),
        }
    })
    .collect();
  if allowed.is_empty() && resolved > 0 {
    return Err(Fail::Error(match lan {
      true => format!("`{host}` resolved only to loopback, link-local or reserved addresses"),
      false => format!(
        "`{host}` resolved only to private or reserved addresses; grant the address itself as a literal IP, or the name in capabilities.network.lan_hosts, to reach it"
      ),
    }));
  }
  Ok(allowed)
}

/// Resolves `host` and keeps what it may reach
pub(crate) async fn resolve(host: &str, port: u16, lan: bool) -> Res<Vec<SocketAddr>> {
  let bare = host.trim_start_matches('[').trim_end_matches(']');
  let addrs = tokio::net::lookup_host((bare, port))
    .await
    .map_err(|e| Fail::Error(format!("resolving `{host}` failed: {e}")))?;
  reachable(host, lan, addrs.collect())
}

/// A TCP connection to a reachable address of `host`, within [`TIMEOUT`]
pub(crate) async fn tcp(host: &str, port: u16, lan: bool) -> Res<TcpStream> {
  let connect = async {
    let mut last = None;
    for addr in resolve(host, port, lan).await? {
      match TcpStream::connect(addr).await {
        Ok(stream) => return Ok(stream),
        Err(e) => last = Some(e),
      }
    }
    Err(Fail::Error(format!(
      "connecting to {host}:{port} failed: {}",
      last.map_or("no address".into(), |e| e.to_string())
    )))
  };
  timeout(TIMEOUT, connect)
    .await
    .map_err(|_| Fail::Error(format!("connecting to {host}:{port} timed out")))?
}

/// The granted canonical path of a unix socket
pub(crate) fn unix_path(ctx: &Ctx<'_>, path: &str) -> Result<std::path::PathBuf> {
  plugin(ctx)?
    .capabilities
    .unix_socket(Path::new(path))
    .ok_or_else(|| {
      Exception::throw_type(
        ctx,
        &format!("unix socket `{path}` is not granted; add it to capabilities.network.unix"),
      )
    })
}

pub(crate) trait Stream: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> Stream for T {}

type Writer = WriteHalf<Box<dyn Stream>>;

/// A connected socket
#[derive(Trace, JsLifetime)]
#[rquickjs::class]
pub struct Socket {
  #[qjs(skip_trace)]
  reader: Arc<Mutex<ReadHalf<Box<dyn Stream>>>>,
  #[qjs(skip_trace)]
  writer: Arc<Mutex<Option<Writer>>>,
  #[qjs(skip_trace)]
  closed: watch::Sender<bool>,
}

impl Socket {
  fn new(stream: Box<dyn Stream>) -> Self {
    let (reader, writer) = tokio::io::split(stream);
    Self {
      reader: Arc::new(Mutex::new(reader)),
      writer: Arc::new(Mutex::new(Some(writer))),
      closed: watch::channel(false).0,
    }
  }
}

#[rquickjs::methods]
impl Socket {
  /// Up to `max_bytes`; null at EOF or after close
  pub fn read<'js>(&self, ctx: Ctx<'js>, max_bytes: Opt<f64>) -> Result<Promise<'js>> {
    let max = max_bytes.0.unwrap_or(CHUNK as f64);
    if !(1.0..=MAX_CHUNK as f64).contains(&max) {
      return Err(Exception::throw_range(
        &ctx,
        &format!("socket.read(maxBytes) must be 1 to {MAX_CHUNK}"),
      ));
    }
    let (reader, mut closed) = (self.reader.clone(), self.closed.subscribe());
    promise(
      &ctx,
      io(async move {
        let read = async {
          let mut buf = vec![0; max as usize];
          let n = reader.lock().await.read(&mut buf).await;
          let n = n.map_err(|e| Fail::Error(format!("reading the socket failed: {e}")))?;
          buf.truncate(n);
          Ok(Null((n > 0).then_some(Bytes(buf))))
        };
        tokio::select! {
          biased;
          _ = closed.wait_for(|c| *c) => Ok(Null(None)),
          read = read => read,
        }
      }),
    )
  }

  /// Strings are sent as UTF-8
  pub fn write<'js>(&self, ctx: Ctx<'js>, data: Value<'js>) -> Result<Promise<'js>> {
    let data = bytes(&data, "socket.write(data)")?;
    if data.len() > MAX_CHUNK {
      return Err(Exception::throw_range(
        &ctx,
        &format!("socket.write(data) exceeded the {MAX_CHUNK} byte limit"),
      ));
    }
    let writer = self.writer.clone();
    promise(
      &ctx,
      io(async move {
        let mut writer = writer.lock().await;
        let writer = writer
          .as_mut()
          .ok_or_else(|| Fail::Error("the socket is closed".into()))?;
        let write = async {
          writer.write_all(&data).await?;
          writer.flush().await
        };
        write
          .await
          .map_err(|e| Fail::Error(format!("writing the socket failed: {e}")))
      }),
    )
  }

  pub fn close(&self) {
    self.closed.send_replace(true);
    let writer = self.writer.clone();
    super::io::detach(async move {
      if let Some(mut writer) = writer.lock().await.take() {
        let _ = writer.shutdown().await;
      }
    });
  }
}

impl Drop for Socket {
  fn drop(&mut self) {
    self.closed.send_replace(true);
  }
}

/// `connect(host, port)` or `connect({ path })`
fn connect<'js>(ctx: Ctx<'js>, target: Value<'js>, port: Opt<Value<'js>>) -> Result<Promise<'js>> {
  if let Some(o) = target.as_object().filter(|_| !target.is_string()) {
    let path: Option<String> = o.get("path")?;
    let Some(path) = path else {
      return Err(Exception::throw_type(
        &ctx,
        "net.connect(target) needs a host and port, or { path }",
      ));
    };
    let path = unix_path(&ctx, &path)?;
    let stream = io(async move {
      let connect = UnixStream::connect(&path);
      let stream = timeout(TIMEOUT, connect)
        .await
        .map_err(|_| Fail::Error(format!("connecting to {} timed out", path.display())))?
        .map_err(|e| Fail::Error(format!("connecting to {} failed: {e}", path.display())))?;
      Ok(Box::new(stream) as Box<dyn Stream>)
    });
    return promise(&ctx, async move { Ok(Socket::new(stream.await?)) });
  }
  let host: String = target.get()?;
  let port = match port.0.and_then(|p| p.as_number()) {
    Some(p) if (1.0..=65535.0).contains(&p) && p.fract() == 0.0 => p as u16,
    _ => {
      return Err(Exception::throw_type(
        &ctx,
        "net.connect(host, port) needs a port from 1 to 65535",
      ));
    }
  };
  let caps = &plugin(&ctx)?.capabilities;
  if !caps.may_reach(&host) {
    return Err(Exception::throw_type(
      &ctx,
      &format!("`{host}` is not granted; add it to capabilities.network.hosts"),
    ));
  }
  let lan = caps.may_reach_lan(&host);
  let stream = io(async move {
    let stream = tcp(&host, port, lan).await?;
    Ok(Box::new(stream) as Box<dyn Stream>)
  });
  promise(&ctx, async move { Ok(Socket::new(stream.await?)) })
}

pub struct NetModule;

impl ModuleDef for NetModule {
  fn declare<'js>(decl: &Declarations<'js>) -> Result<()> {
    decl.declare("connect")?.declare("default")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> Result<()> {
    let f = Function::new(ctx.clone(), connect)?;
    let all = Object::new(ctx.clone())?;
    all.set("connect", f.clone())?;
    exports.export("connect", f)?;
    exports.export("default", all)?;
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn ip(s: &str) -> IpAddr {
    s.parse().unwrap()
  }

  #[test]
  fn public_addresses_exclude_every_special_range() {
    let cases = [
      ("8.8.8.8", true),
      ("1.1.1.1", true),
      ("10.0.0.1", false),
      ("172.16.0.1", false),
      ("192.168.1.1", false),
      ("127.0.0.1", false),
      ("169.254.1.1", false),
      ("0.0.0.0", false),
      ("240.0.0.1", false),
      ("100.63.255.255", true),
      ("100.64.0.0", false),
      ("100.127.255.255", false),
      ("100.128.0.0", true),
      ("198.17.255.255", true),
      ("198.18.0.0", false),
      ("198.19.255.255", false),
      ("198.20.0.0", true),
      ("192.0.2.1", false),
      ("198.51.100.1", false),
      ("203.0.113.1", false),
      ("::1", false),
      ("::", false),
      ("fe80::1", false),
      ("fc00::1", false),
      ("2001:4860:4860::8888", true),
      ("2001:db8::1", false),
      ("2001:0:4136:e378:8000:63bf:3fff:fdd2", false),
      ("::ffff:8.8.8.8", true),
      ("::ffff:10.0.0.1", false),
      ("::ffff:127.0.0.1", false),
      ("64:ff9b::808:808", true),
      ("64:ff9b::a00:1", false),
      ("64:ff9b:1::1", false),
      ("2002:808:808::1", true),
      ("2002:a00:1::1", false),
      ("2002:7f00:1::1", false),
    ];
    for (addr, want) in cases {
      assert_eq!(is_public(ip(addr)), want, "{addr}");
    }
  }

  #[test]
  fn lan_addresses_are_private_not_loopback_or_link_local() {
    let cases = [
      ("10.1.2.3", true),
      ("172.16.0.1", true),
      ("172.32.0.1", false),
      ("192.168.0.1", true),
      ("100.64.0.1", true),
      ("127.0.0.1", false),
      ("169.254.0.1", false),
      ("8.8.8.8", false),
      ("fd00::1", true),
      ("fe80::1", false),
      ("::1", false),
      ("::ffff:192.168.0.1", true),
      ("::ffff:8.8.8.8", false),
    ];
    for (addr, want) in cases {
      assert_eq!(is_lan(ip(addr)), want, "{addr}");
    }
  }

  fn at(addr: &str) -> Vec<SocketAddr> {
    vec![SocketAddr::new(ip(addr), 80)]
  }

  #[test]
  fn reachable_filters_names_but_not_literals() {
    // a name must resolve to public addresses, or LAN ones when it is a LAN host
    assert!(reachable("example.com", false, at("10.0.0.1")).is_err());
    assert_eq!(
      reachable("example.com", false, at("8.8.8.8"))
        .unwrap()
        .len(),
      1
    );
    assert_eq!(
      reachable("nas.home", true, at("192.168.0.2"))
        .unwrap()
        .len(),
      1
    );
    assert!(reachable("nas.home", true, at("127.0.0.1")).is_err());
    assert!(reachable("nas.home", true, at("169.254.0.1")).is_err());
    // a rebound name keeps only its public address
    let mixed = [at("10.0.0.1"), at("8.8.8.8")].concat();
    assert_eq!(
      reachable("example.com", false, mixed).unwrap(),
      at("8.8.8.8")
    );
    // literals reach what they name
    assert_eq!(
      reachable("10.0.0.1", false, at("10.0.0.1")).unwrap().len(),
      1
    );
    assert_eq!(reachable("[::1]", false, at("::1")).unwrap().len(), 1);
    // localhost reaches loopback only
    assert_eq!(
      reachable("LocalHost", false, at("127.0.0.1"))
        .unwrap()
        .len(),
      1
    );
    assert!(reachable("localhost", false, at("8.8.8.8")).is_err());
    // nothing resolved is not a refusal
    assert!(reachable("example.com", false, vec![]).unwrap().is_empty());
  }
}
