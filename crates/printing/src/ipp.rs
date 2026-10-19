//! A minimal IPP/2.0 codec (RFC 8010) and HTTP transport over CUPS's local
//! socket. It covers the value types CUPS answers with; anything else is
//! kept as raw bytes.

use std::{path::Path, time::Duration};

use anyhow::{Context, Result, bail};
use futures_lite::{AsyncReadExt, AsyncWriteExt};

pub mod tag {
  pub const OPERATION: u8 = 0x01;
  pub const JOB: u8 = 0x02;
  pub const END: u8 = 0x03;
  pub const PRINTER: u8 = 0x04;
  pub const SUBSCRIPTION: u8 = 0x06;

  pub const INTEGER: u8 = 0x21;
  pub const BOOLEAN: u8 = 0x22;
  pub const ENUM: u8 = 0x23;
  pub const RANGE: u8 = 0x33;
  pub const TEXT: u8 = 0x41;
  pub const NAME: u8 = 0x42;
  pub const KEYWORD: u8 = 0x44;
  pub const URI: u8 = 0x45;
  pub const CHARSET: u8 = 0x47;
  pub const LANGUAGE: u8 = 0x48;
  pub const MIME: u8 = 0x49;
}

pub mod op {
  pub const PRINT_JOB: u16 = 0x0002;
  pub const CANCEL_JOB: u16 = 0x0008;
  pub const GET_JOBS: u16 = 0x000A;
  pub const PAUSE_PRINTER: u16 = 0x0010;
  pub const RESUME_PRINTER: u16 = 0x0011;
  pub const CREATE_PRINTER_SUBSCRIPTION: u16 = 0x0016;
  pub const RENEW_SUBSCRIPTION: u16 = 0x0018;
  pub const CANCEL_SUBSCRIPTION: u16 = 0x001B;
  pub const CUPS_GET_PRINTERS: u16 = 0x4002;
  pub const CUPS_ADD_MODIFY_PRINTER: u16 = 0x4003;
  pub const CUPS_GET_PPDS: u16 = 0x400C;
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
  Int(i32),
  Bool(bool),
  Enum(i32),
  Range(i32, i32),
  /// text, name, keyword, uri, charset, language and mime type, by tag
  Str(u8, String),
  Other(u8, Vec<u8>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Attr {
  pub name: String,
  pub values: Vec<Value>,
}

impl Attr {
  pub fn new(name: &str, value: Value) -> Self {
    Self {
      name: name.into(),
      values: vec![value],
    }
  }

  pub fn many(name: &str, values: impl IntoIterator<Item = Value>) -> Self {
    Self {
      name: name.into(),
      values: values.into_iter().collect(),
    }
  }

  pub fn int(&self) -> Option<i32> {
    self.ints().next()
  }

  pub fn ints(&self) -> impl Iterator<Item = i32> + '_ {
    self.values.iter().filter_map(|v| match v {
      Value::Int(i) | Value::Enum(i) => Some(*i),
      _ => None,
    })
  }

  pub fn bool(&self) -> Option<bool> {
    self.values.iter().find_map(|v| match v {
      Value::Bool(b) => Some(*b),
      _ => None,
    })
  }

  pub fn str(&self) -> Option<&str> {
    self.strs().next()
  }

  pub fn strs(&self) -> impl Iterator<Item = &str> + '_ {
    self.values.iter().filter_map(|v| match v {
      Value::Str(_, s) => Some(s.as_str()),
      _ => None,
    })
  }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Group {
  pub tag: u8,
  pub attrs: Vec<Attr>,
}

impl Group {
  pub fn new(tag: u8, attrs: Vec<Attr>) -> Self {
    Self { tag, attrs }
  }

  pub fn get(&self, name: &str) -> Option<&Attr> {
    self.attrs.iter().find(|a| a.name == name)
  }
}

/// A request (`code` is the operation) or a response (`code` is the status).
#[derive(Clone, Debug, PartialEq)]
pub struct Message {
  pub code: u16,
  pub request_id: u32,
  pub groups: Vec<Group>,
  pub data: Vec<u8>,
}

impl Message {
  /// A request with the operation attributes every request starts with.
  pub fn request(op: u16, mut operation: Vec<Attr>) -> Self {
    let mut attrs = vec![
      Attr::new(
        "attributes-charset",
        Value::Str(tag::CHARSET, "utf-8".into()),
      ),
      Attr::new(
        "attributes-natural-language",
        Value::Str(tag::LANGUAGE, "en".into()),
      ),
    ];
    attrs.append(&mut operation);
    Self {
      code: op,
      request_id: 1,
      groups: vec![Group::new(tag::OPERATION, attrs)],
      data: Vec::new(),
    }
  }

  pub fn groups(&self, tag: u8) -> impl Iterator<Item = &Group> + '_ {
    self.groups.iter().filter(move |g| g.tag == tag)
  }

  #[cfg(test)]
  pub fn operation(&self, name: &str) -> Option<&Attr> {
    self.groups(tag::OPERATION).find_map(|g| g.get(name))
  }

  pub fn encode(&self) -> Vec<u8> {
    let mut out = vec![2, 0];
    out.extend(self.code.to_be_bytes());
    out.extend(self.request_id.to_be_bytes());
    for group in &self.groups {
      out.push(group.tag);
      for attr in &group.attrs {
        for (i, value) in attr.values.iter().enumerate() {
          // additional values of the same attribute have an empty name
          let name = if i == 0 { attr.name.as_bytes() } else { &[] };
          let (tag, bytes) = match value {
            Value::Int(v) => (tag::INTEGER, v.to_be_bytes().to_vec()),
            Value::Enum(v) => (tag::ENUM, v.to_be_bytes().to_vec()),
            Value::Bool(v) => (tag::BOOLEAN, vec![*v as u8]),
            Value::Range(a, b) => (tag::RANGE, [a.to_be_bytes(), b.to_be_bytes()].concat()),
            Value::Str(tag, s) => (*tag, s.as_bytes().to_vec()),
            Value::Other(tag, b) => (*tag, b.clone()),
          };
          out.push(tag);
          out.extend((name.len() as u16).to_be_bytes());
          out.extend(name);
          out.extend((bytes.len() as u16).to_be_bytes());
          out.extend(bytes);
        }
      }
    }
    out.push(tag::END);
    out.extend(&self.data);
    out
  }

  pub fn parse(bytes: &[u8]) -> Result<Self> {
    let mut r = Reader(bytes);
    r.take(2).context("IPP version")?;
    let code = u16::from_be_bytes(r.array()?);
    let request_id = u32::from_be_bytes(r.array()?);
    let mut groups: Vec<Group> = Vec::new();
    loop {
      let tag = r
        .byte()
        .context("IPP message ends before end-of-attributes")?;
      match tag {
        tag::END => break,
        0x00..=0x0F => groups.push(Group::new(tag, Vec::new())),
        _ => {
          let name = String::from_utf8_lossy(r.sized()?).into_owned();
          let raw = r.sized()?;
          let value = match tag {
            tag::INTEGER => Value::Int(i32::from_be_bytes(raw.try_into()?)),
            tag::ENUM => Value::Enum(i32::from_be_bytes(raw.try_into()?)),
            tag::BOOLEAN => Value::Bool(raw.first().is_some_and(|b| *b != 0)),
            tag::RANGE if raw.len() == 8 => Value::Range(
              i32::from_be_bytes(raw[..4].try_into()?),
              i32::from_be_bytes(raw[4..].try_into()?),
            ),
            0x41..=0x49 => Value::Str(tag, String::from_utf8_lossy(raw).into_owned()),
            _ => Value::Other(tag, raw.to_vec()),
          };
          let group = groups.last_mut().context("IPP value outside a group")?;
          match group.attrs.last_mut() {
            Some(attr) if name.is_empty() => attr.values.push(value),
            _ => group.attrs.push(Attr::new(&name, value)),
          }
        }
      }
    }
    Ok(Self {
      code,
      request_id,
      groups,
      data: r.0.to_vec(),
    })
  }
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
  fn take(&mut self, n: usize) -> Result<&'a [u8]> {
    if self.0.len() < n {
      bail!("IPP message truncated");
    }
    let (head, tail) = self.0.split_at(n);
    self.0 = tail;
    Ok(head)
  }

  fn byte(&mut self) -> Result<u8> {
    Ok(self.take(1)?[0])
  }

  fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
    Ok(self.take(N)?.try_into()?)
  }

  fn sized(&mut self) -> Result<&'a [u8]> {
    let len = u16::from_be_bytes(self.array()?) as usize;
    self.take(len)
  }
}

/// What a failed request means for the caller.
#[derive(Debug, PartialEq)]
pub enum IppError {
  /// CUPS wants an administrator, retry through polkit
  Unauthorized,
  /// the status code and CUPS' status-message
  Status(u16, String),
}

impl std::fmt::Display for IppError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Unauthorized => write!(f, "not authorized"),
      Self::Status(code, message) if message.is_empty() => {
        write!(f, "CUPS answered status {code:#06x}")
      }
      Self::Status(code, message) => write!(f, "CUPS: {message} ({code:#06x})"),
    }
  }
}

impl std::error::Error for IppError {}

/// Sends `request` to CUPS over its domain socket and returns the response.
/// Fails with [`IppError`] when CUPS refuses it.
pub async fn send(sock: &Path, request: &Message) -> Result<Message> {
  send_within(sock, request, Duration::from_secs(30)).await
}

/// [`send`], giving up after `timeout`.
pub async fn send_within(sock: &Path, request: &Message, timeout: Duration) -> Result<Message> {
  futures_lite::future::or(exchange(sock, request), async {
    smol::Timer::after(timeout).await;
    bail!("CUPS did not answer within {} s", timeout.as_secs())
  })
  .await
}

async fn exchange(sock: &Path, request: &Message) -> Result<Message> {
  let body = request.encode();
  let (mut status, mut reply) = http_body(&post(sock, &body, None).await?)?;
  // cupsd's socket trusts the uid it sees: an admin user needs no password
  if status == 401
    && let Ok(user) = std::env::var("USER")
  {
    let auth = format!("PeerCred {user}");
    (status, reply) = http_body(&post(sock, &body, Some(&auth)).await?)?;
  }
  let body = reply;
  if status == 401 || status == 403 {
    return Err(IppError::Unauthorized.into());
  }
  if status != 200 {
    bail!("CUPS answered HTTP {status}");
  }
  let response = Message::parse(&body)?;
  match response.code {
    0x0000..=0x00FF => Ok(response),
    // client-error-forbidden, -not-authenticated, -not-authorized
    0x0401..=0x0403 => Err(IppError::Unauthorized.into()),
    code => {
      let message = response
        .groups(tag::OPERATION)
        .find_map(|g| g.get("status-message")?.str().map(String::from))
        .unwrap_or_default();
      Err(IppError::Status(code, message).into())
    }
  }
}

async fn post(sock: &Path, body: &[u8], auth: Option<&str>) -> Result<Vec<u8>> {
  let mut stream = smol::net::unix::UnixStream::connect(sock)
    .await
    .with_context(|| format!("connecting to {}", sock.display()))?;
  let auth = auth
    .map(|a| format!("Authorization: {a}\r\n"))
    .unwrap_or_default();
  let head = format!(
    "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/ipp\r\nContent-Length: {}\r\n{auth}Connection: close\r\n\r\n",
    body.len()
  );
  stream.write_all(head.as_bytes()).await?;
  stream.write_all(body).await?;
  stream.flush().await?;
  let mut raw = Vec::new();
  let mut buf = [0; 8192];
  loop {
    let n = stream.read(&mut buf).await?;
    if n == 0 {
      break;
    }
    raw.extend(&buf[..n]);
    // cupsd keeps the connection open after a 401 despite `Connection: close`
    if complete(&raw) {
      break;
    }
  }
  Ok(raw)
}

/// Whether `raw` is a whole response, by its Content-Length or last chunk.
pub fn complete(raw: &[u8]) -> bool {
  let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") else {
    return false;
  };
  let head = String::from_utf8_lossy(&raw[..end]).to_ascii_lowercase();
  let framed =
    head.contains("\r\ncontent-length:") || head.contains("\r\ntransfer-encoding: chunked");
  framed && http_body(raw).is_ok()
}

/// A whole HTTP/1.1 response, its body decoded.
pub struct HttpResponse {
  pub status: u16,
  pub headers: Vec<(String, String)>,
  pub body: Vec<u8>,
}

impl HttpResponse {
  pub fn header(&self, name: &str) -> Option<&str> {
    self
      .headers
      .iter()
      .find_map(|(k, v)| k.eq_ignore_ascii_case(name).then_some(v.as_str()))
  }
}

pub fn parse_response(raw: &[u8]) -> Result<HttpResponse> {
  let end = raw
    .windows(4)
    .position(|w| w == b"\r\n\r\n")
    .context("HTTP response without a header end")?;
  let head = String::from_utf8_lossy(&raw[..end]);
  let body = &raw[end + 4..];
  let mut lines = head.split("\r\n");
  let status = lines
    .next()
    .and_then(|l| l.split(' ').nth(1))
    .and_then(|s| s.parse().ok())
    .context("HTTP status line")?;
  let headers = lines
    .filter_map(|l| {
      let (k, v) = l.split_once(':')?;
      Some((k.trim().to_string(), v.trim().to_string()))
    })
    .collect();
  let mut response = HttpResponse {
    status,
    headers,
    body: Vec::new(),
  };
  response.body = match response.header("transfer-encoding") {
    Some(v) if v.eq_ignore_ascii_case("chunked") => dechunk(body)?,
    _ => {
      let len = response
        .header("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(body.len());
      body.get(..len).context("HTTP body truncated")?.to_vec()
    }
  };
  Ok(response)
}

/// Splits a whole HTTP/1.1 response into its status and decoded body.
pub fn http_body(raw: &[u8]) -> Result<(u16, Vec<u8>)> {
  let response = parse_response(raw)?;
  Ok((response.status, response.body))
}

fn dechunk(mut body: &[u8]) -> Result<Vec<u8>> {
  let mut out = Vec::new();
  loop {
    let line = body
      .windows(2)
      .position(|w| w == b"\r\n")
      .context("chunk size line")?;
    let size = std::str::from_utf8(&body[..line])?;
    let size = usize::from_str_radix(size.split(';').next().unwrap_or("").trim(), 16)?;
    body = &body[line + 2..];
    if size == 0 {
      return Ok(out);
    }
    out.extend(body.get(..size).context("chunk truncated")?);
    body = body.get(size + 2..).context("chunk truncated")?;
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn round_trips() {
    let mut msg = Message::request(
      op::GET_JOBS,
      vec![
        Attr::new(
          "printer-uri",
          Value::Str(tag::URI, "ipp://localhost/".into()),
        ),
        Attr::many(
          "requested-attributes",
          ["job-id", "job-state"].map(|s| Value::Str(tag::KEYWORD, s.into())),
        ),
      ],
    );
    msg.groups.push(Group::new(
      tag::JOB,
      vec![
        Attr::new("job-id", Value::Int(7)),
        Attr::new("job-state", Value::Enum(5)),
        Attr::new("accepting", Value::Bool(true)),
        Attr::new("copies-supported", Value::Range(1, 99)),
        Attr::new("odd", Value::Other(0x30, vec![1, 2])),
      ],
    ));
    msg.data = b"%PDF".to_vec();
    let parsed = Message::parse(&msg.encode()).unwrap();
    assert_eq!(parsed, msg);
    let jobs = parsed.groups(tag::JOB).next().unwrap();
    assert_eq!(jobs.get("job-state").unwrap().int(), Some(5));
    let keywords: Vec<_> = parsed
      .operation("requested-attributes")
      .unwrap()
      .strs()
      .collect();
    assert_eq!(keywords, ["job-id", "job-state"]);
  }

  #[test]
  fn truncated_messages_fail() {
    let bytes = Message::request(op::GET_JOBS, vec![]).encode();
    for len in [0, 3, 9, bytes.len() - 1] {
      assert!(Message::parse(&bytes[..len]).is_err(), "len {len}");
    }
  }

  #[test]
  fn reads_http_bodies() {
    let plain = b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nabcdef";
    assert_eq!(http_body(plain).unwrap(), (200, b"abc".to_vec()));
    let chunked =
      b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2;x=y\r\nde\r\n0\r\n\r\n";
    assert_eq!(http_body(chunked).unwrap(), (200, b"abcde".to_vec()));
    assert_eq!(
      http_body(b"HTTP/1.1 401 Unauthorized\r\n\r\n").unwrap().0,
      401
    );
    assert!(http_body(b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nab").is_err());

    assert!(complete(plain));
    assert!(complete(chunked));
    assert!(!complete(&chunked[..chunked.len() - 5]));
    assert!(!complete(b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nab"));
    // unframed bodies end with the connection
    assert!(!complete(b"HTTP/1.1 200 OK\r\n\r\nab"));
  }
}
