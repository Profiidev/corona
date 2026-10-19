//! HTTPS to scanners. Their certificates are self-signed, so no CA can
//! vouch for them: the first certificate a scanner shows is pinned, and a
//! different one later is refused (trust on first use).

use std::{
  io::{Read, Write},
  net::{TcpStream, ToSocketAddrs},
  path::{Path, PathBuf},
  pin::Pin,
  sync::Arc,
  time::Duration,
};

use anyhow::{Context, Result, bail};
use futures_lite::AsyncReadExt;
use gpui_kit::http_client::{
  AsyncBody, HttpClient, Request, Response, Url,
  http::{HeaderValue, request::Parts},
};
use rustls::{
  ClientConfig, ClientConnection, DigitallySignedStruct, SignatureScheme, StreamOwned,
  client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
  crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature},
  pki_types::{CertificateDer, ServerName, UnixTime},
};

use crate::ipp::{HttpResponse, complete, parse_response};

const CONNECT: Duration = Duration::from_secs(10);
/// a high resolution page can take a while before its first byte
const READ: Duration = Duration::from_secs(120);

/// Where pinned scanner certificates are kept, one `name<TAB>sha256` a line.
pub fn pins_file() -> PathBuf {
  dirs::state_dir()
    .or_else(dirs::cache_dir)
    .unwrap_or_else(std::env::temp_dir)
    .join("corona/scanner-certs")
}

/// An [`HttpClient`] for one scanner, pinning its certificate under `key`.
pub struct PinnedClient {
  key: String,
  pins: PathBuf,
  user_agent: HeaderValue,
}

impl PinnedClient {
  pub fn new(key: &str, pins: &Path) -> Self {
    Self {
      key: key.replace(['\t', '\n'], " "),
      pins: pins.to_path_buf(),
      user_agent: HeaderValue::from_static(concat!("corona/", env!("CARGO_PKG_VERSION"))),
    }
  }
}

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

impl HttpClient for PinnedClient {
  fn user_agent(&self) -> Option<&HeaderValue> {
    Some(&self.user_agent)
  }

  fn proxy(&self) -> Option<&Url> {
    None
  }

  fn send(&self, req: Request<AsyncBody>) -> BoxFuture<Result<Response<AsyncBody>>> {
    let (key, pins, agent) = (self.key.clone(), self.pins.clone(), self.user_agent.clone());
    Box::pin(async move {
      let (parts, mut body) = req.into_parts();
      let mut bytes = Vec::new();
      body.read_to_end(&mut bytes).await?;
      let response = smol::unblock(move || fetch(&key, &pins, &agent, &parts, &bytes)).await?;
      let mut builder = Response::builder().status(response.status);
      for (name, value) in &response.headers {
        // the body is already decoded
        if !name.eq_ignore_ascii_case("transfer-encoding")
          && !name.eq_ignore_ascii_case("content-length")
        {
          builder = builder.header(name, value);
        }
      }
      Ok(builder.body(AsyncBody::from(response.body))?)
    })
  }
}

fn fetch(
  key: &str,
  pins: &Path,
  agent: &HeaderValue,
  parts: &Parts,
  body: &[u8],
) -> Result<HttpResponse> {
  let host = parts.uri.host().context("URL without a host")?;
  let port = parts.uri.port_u16().unwrap_or(443);
  let name = ServerName::try_from(host.trim_matches(['[', ']']).to_string())?;
  let provider = Arc::new(rustls::crypto::ring::default_provider());
  let verifier = Pinning {
    key: key.to_string(),
    pins: pins.to_path_buf(),
    provider: provider.clone(),
  };
  let config = ClientConfig::builder_with_provider(provider)
    .with_safe_default_protocol_versions()?
    .dangerous()
    .with_custom_certificate_verifier(Arc::new(verifier))
    .with_no_client_auth();

  let addr = (host.trim_matches(['[', ']']), port)
    .to_socket_addrs()?
    .next()
    .with_context(|| format!("no address for {host}"))?;
  let tcp =
    TcpStream::connect_timeout(&addr, CONNECT).with_context(|| format!("connecting to {addr}"))?;
  tcp.set_read_timeout(Some(READ))?;
  tcp.set_write_timeout(Some(CONNECT))?;
  let mut tls = StreamOwned::new(ClientConnection::new(Arc::new(config), name)?, tcp);

  let path = parts.uri.path_and_query().map_or("/", |p| p.as_str());
  let mut head = format!(
    "{} {path} HTTP/1.1\r\nHost: {host}:{port}\r\n",
    parts.method
  );
  for (name, value) in &parts.headers {
    head.push_str(&format!(
      "{name}: {}\r\n",
      value.to_str().unwrap_or_default()
    ));
  }
  head.push_str(&format!(
    "User-Agent: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
    agent.to_str().unwrap_or_default(),
    body.len()
  ));
  tls.write_all(head.as_bytes())?;
  tls.write_all(body)?;
  tls.flush()?;

  let mut raw = Vec::new();
  let mut buf = [0; 16 * 1024];
  loop {
    match tls.read(&mut buf) {
      Ok(0) => break,
      Ok(n) => {
        raw.extend(&buf[..n]);
        if complete(&raw) {
          break;
        }
      }
      // embedded servers often hang up without a TLS close_notify
      Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof && !raw.is_empty() => break,
      Err(e) => return Err(e).with_context(|| format!("reading from {host}")),
    }
  }
  parse_response(&raw)
}

fn fingerprint(cert: &[u8]) -> String {
  let digest = ring::digest::digest(&ring::digest::SHA256, cert);
  digest.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

/// Accepts `print` for `key` if it is the pinned one, pinning it when none is.
fn pin(pins: &Path, key: &str, print: &str) -> Result<()> {
  let known = std::fs::read_to_string(pins).unwrap_or_default();
  let pinned = known.lines().find_map(|line| {
    line
      .split_once('\t')
      .filter(|(k, _)| *k == key)
      .map(|(_, v)| v)
  });
  match pinned {
    Some(pinned) if pinned == print => Ok(()),
    Some(_) => bail!(
      "the certificate of {key} changed since it was first used. If the scanner was reset, remove its line from {}",
      pins.display()
    ),
    None => {
      if let Some(dir) = pins.parent() {
        std::fs::create_dir_all(dir)?;
      }
      let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(pins)?;
      writeln!(file, "{key}\t{print}")?;
      Ok(())
    }
  }
}

#[derive(Debug)]
struct Pinning {
  key: String,
  pins: PathBuf,
  provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for Pinning {
  fn verify_server_cert(
    &self,
    end_entity: &CertificateDer<'_>,
    _intermediates: &[CertificateDer<'_>],
    _server_name: &ServerName<'_>,
    _ocsp_response: &[u8],
    _now: UnixTime,
  ) -> Result<ServerCertVerified, rustls::Error> {
    pin(&self.pins, &self.key, &fingerprint(end_entity))
      .map(|()| ServerCertVerified::assertion())
      .map_err(|e| rustls::Error::General(e.to_string()))
  }

  // the signatures still prove the scanner holds the pinned certificate's key
  fn verify_tls12_signature(
    &self,
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
  ) -> Result<HandshakeSignatureValid, rustls::Error> {
    verify_tls12_signature(
      message,
      cert,
      dss,
      &self.provider.signature_verification_algorithms,
    )
  }

  fn verify_tls13_signature(
    &self,
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
  ) -> Result<HandshakeSignatureValid, rustls::Error> {
    verify_tls13_signature(
      message,
      cert,
      dss,
      &self.provider.signature_verification_algorithms,
    )
  }

  fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
    self
      .provider
      .signature_verification_algorithms
      .supported_schemes()
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use std::net::TcpListener;

  use futures_lite::future::block_on;
  use rustls::{ServerConfig, ServerConnection, pki_types::PrivateKeyDer};

  use super::*;

  /// A TLS server on 127.0.0.1 with test certificate `which` ("a" or "b"),
  /// answering every request with `answer(method, path, body)`.
  pub fn serve(
    which: &str,
    answer: impl Fn(&str, &str, &str) -> (u16, Vec<(String, String)>, Vec<u8>) + Send + 'static,
  ) -> u16 {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata");
    let cert = std::fs::read(dir.join(format!("cert-{which}.der"))).unwrap();
    let key = std::fs::read(dir.join(format!("key-{which}.der"))).unwrap();
    let config =
      ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
          vec![CertificateDer::from(cert)],
          PrivateKeyDer::Pkcs8(key.into()),
        )
        .unwrap();
    let config = Arc::new(config);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
      for tcp in listener.incoming() {
        let Ok(tcp) = tcp else { break };
        let Ok(conn) = ServerConnection::new(config.clone()) else {
          continue;
        };
        let mut tls = StreamOwned::new(conn, tcp);
        let mut raw = Vec::new();
        let mut buf = [0; 4096];
        // headers, then as much body as Content-Length says
        let request = loop {
          let n = match tls.read(&mut buf) {
            Ok(0) | Err(_) => break None,
            Ok(n) => n,
          };
          raw.extend(&buf[..n]);
          let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") else {
            continue;
          };
          let head = String::from_utf8_lossy(&raw[..end]).to_string();
          let len: usize = head
            .lines()
            .find_map(|l| l.strip_prefix("Content-Length: ")?.trim().parse().ok())
            .unwrap_or(0);
          if raw.len() >= end + 4 + len {
            let body = String::from_utf8_lossy(&raw[end + 4..end + 4 + len]).to_string();
            break Some((head, body));
          }
        };
        let Some((head, body)) = request else {
          continue;
        };
        let mut first = head.lines().next().unwrap_or("").split(' ');
        let (method, path) = (first.next().unwrap_or(""), first.next().unwrap_or(""));
        let (status, headers, reply) = answer(method, path, &body);
        let mut out = format!("HTTP/1.1 {status} X\r\nContent-Length: {}\r\n", reply.len());
        for (k, v) in headers {
          out.push_str(&format!("{k}: {v}\r\n"));
        }
        out.push_str("\r\n");
        tls.write_all(out.as_bytes()).ok();
        tls.write_all(&reply).ok();
        tls.conn.send_close_notify();
        tls.flush().ok();
      }
    });
    port
  }

  fn get(client: &PinnedClient, port: u16) -> Result<(u16, String)> {
    let url = format!("https://127.0.0.1:{port}/eSCL/ScannerStatus");
    let mut response = block_on(client.get(&url, AsyncBody::empty(), false))?;
    let mut body = String::new();
    block_on(response.body_mut().read_to_string(&mut body))?;
    Ok((response.status().as_u16(), body))
  }

  #[test]
  fn pins_the_first_certificate() {
    let dir = tempfile::tempdir().unwrap();
    let pins = dir.path().join("state/corona/scanner-certs");
    let ok = |_: &str, _: &str, _: &str| (200, vec![], b"<State>Idle</State>".to_vec());
    let (a, b) = (serve("a", ok), serve("b", ok));

    let client = PinnedClient::new("Office\tscanner", &pins);
    assert_eq!(
      get(&client, a).unwrap(),
      (200, "<State>Idle</State>".into())
    );
    let pinned = std::fs::read_to_string(&pins).unwrap();
    assert!(pinned.starts_with("Office scanner\t"), "{pinned}");
    assert_eq!(pinned.lines().count(), 1);
    // the same certificate again is fine, and pins nothing new
    get(&client, a).unwrap();
    assert_eq!(std::fs::read_to_string(&pins).unwrap(), pinned);

    // another certificate for the same scanner is refused
    let err = format!("{:?}", get(&client, b).unwrap_err());
    assert!(err.contains("changed since it was first used"), "{err}");
    // another scanner pins its own
    get(&PinnedClient::new("Lab", &pins), b).unwrap();
    assert_eq!(std::fs::read_to_string(&pins).unwrap().lines().count(), 2);
  }
}
