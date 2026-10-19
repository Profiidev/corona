//! Scanners and network printers announced over DNS-SD, through avahi-daemon.

use std::collections::BTreeMap;

use anyhow::Result;
use zbus::zvariant::OwnedObjectPath;

use crate::state::{NetworkPrinter, Printer, Scanner};

pub const AVAHI: &str = "org.freedesktop.Avahi";
/// eSCL over HTTP and over TLS
pub const SCAN: &str = "_uscan._tcp";
pub const SCANS: &str = "_uscans._tcp";
/// IPP over HTTP and over TLS
pub const IPP: &str = "_ipp._tcp";
pub const IPPS: &str = "_ipps._tcp";
const KINDS: [&str; 4] = [SCAN, SCANS, IPP, IPPS];
const IF_UNSPEC: i32 = -1;
const PROTO_UNSPEC: i32 = -1;
const PROTO_INET: i32 = 0;

#[zbus::proxy(
  interface = "org.freedesktop.Avahi.Server",
  default_service = "org.freedesktop.Avahi",
  default_path = "/"
)]
pub trait Server {
  fn service_browser_new(
    &self,
    interface: i32,
    protocol: i32,
    kind: &str,
    domain: &str,
    flags: u32,
  ) -> zbus::Result<OwnedObjectPath>;

  #[allow(clippy::type_complexity, clippy::too_many_arguments)]
  fn resolve_service(
    &self,
    interface: i32,
    protocol: i32,
    name: &str,
    kind: &str,
    domain: &str,
    aprotocol: i32,
    flags: u32,
  ) -> zbus::Result<(
    i32,
    i32,
    String,
    String,
    String,
    String,
    i32,
    String,
    u16,
    Vec<Vec<u8>>,
    u32,
  )>;
}

/// The body of `ItemNew` and `ItemRemove`.
pub type Item = (i32, i32, String, String, String, u32);

/// A resolved announcement.
#[derive(Clone, Debug, PartialEq)]
pub struct Service {
  pub name: String,
  pub kind: String,
  pub address: String,
  pub port: u16,
  pub txt: Vec<(String, String)>,
}

impl Service {
  pub fn new(name: &str, kind: &str, address: &str, port: u16, txt: &[Vec<u8>]) -> Self {
    let txt = txt
      .iter()
      .filter_map(|entry| {
        let entry = String::from_utf8_lossy(entry);
        let (k, v) = entry.split_once('=')?;
        Some((k.to_ascii_lowercase(), v.to_string()))
      })
      .collect();
    Self {
      name: name.into(),
      kind: kind.into(),
      address: address.into(),
      port,
      txt,
    }
  }

  pub fn record(&self, key: &str) -> Option<&str> {
    let key = key.to_ascii_lowercase();
    self
      .txt
      .iter()
      .find_map(|(k, v)| (*k == key).then_some(v.as_str()))
  }
}

/// Starts browsing for scanners and printers. Subscribe to the browser
/// signals first: Avahi starts emitting as soon as a browser exists.
pub async fn browse(server: &ServerProxy<'_>) -> Result<()> {
  for kind in KINDS {
    server
      .service_browser_new(IF_UNSPEC, PROTO_UNSPEC, kind, "", 0)
      .await?;
  }
  Ok(())
}

pub async fn resolve(server: &ServerProxy<'_>, item: &Item) -> Result<Service> {
  let (interface, protocol, name, kind, domain, _) = item;
  // IPv4 first: an IPv6 link-local address would need a scope in the URL
  let mut resolved = server
    .resolve_service(*interface, *protocol, name, kind, domain, PROTO_INET, 0)
    .await;
  if resolved.is_err() {
    resolved = server
      .resolve_service(*interface, *protocol, name, kind, domain, PROTO_UNSPEC, 0)
      .await;
  }
  let (.., address, port, txt, _) = resolved?;
  Ok(Service::new(name, kind, &address, port, &txt))
}

/// One entry per name, kind, interface and protocol a service was announced on.
pub type Found = BTreeMap<(String, String, i32, i32), Service>;

pub fn key(item: &Item) -> (String, String, i32, i32) {
  (item.2.clone(), item.3.clone(), item.0, item.1)
}

/// One service per name of the given kinds, `preferred` over the other.
fn by_name<'a>(found: &'a Found, preferred: &str, other: &str) -> Vec<&'a Service> {
  let mut by_name: BTreeMap<&str, &Service> = BTreeMap::new();
  for service in found.values() {
    if service.kind != preferred && service.kind != other {
      continue;
    }
    let slot = by_name.entry(&service.name).or_insert(service);
    if slot.kind != preferred && service.kind == preferred {
      *slot = service;
    }
  }
  by_name.into_values().collect()
}

/// Scanners, the plain-HTTP announcement over the TLS one.
pub fn scanners(found: &Found) -> Vec<Scanner> {
  by_name(found, SCAN, SCANS)
    .into_iter()
    .map(crate::escl::scanner)
    .collect()
}

/// Printers, the TLS announcement over the plain one: CUPS speaks both. Queues
/// another CUPS shares, ours included, are left out.
pub fn printers(found: &Found) -> Vec<NetworkPrinter> {
  by_name(found, IPPS, IPP)
    .into_iter()
    // CUPS puts its printer-type in the TXT record of every queue it shares
    .filter(|s| s.record("printer-type").is_none())
    .map(|s| NetworkPrinter {
      name: s.name.clone(),
      model: s.record("ty").map(String::from),
      address: s.address.clone(),
      uri: format!("dnssd://{}.{}.local/", encode(&s.name), s.kind),
      direct_uri: direct_uri(s),
    })
    .collect()
}

/// The printer at its address, e.g. `ipp://10.0.0.7:631/ipp/print`.
fn direct_uri(s: &Service) -> String {
  let scheme = if s.kind == IPPS { "ipps" } else { "ipp" };
  let host = match s.address.contains(':') {
    true => format!("[{}]", s.address),
    false => s.address.clone(),
  };
  let path = s.record("rp").unwrap_or("ipp/print").trim_matches('/');
  format!("{scheme}://{host}:{}/{path}", s.port)
}

/// [`NetworkPrinter::direct_uri`] looked up again now.
pub async fn resolve_printer(conn: &zbus::Connection, printer: &NetworkPrinter) -> Result<String> {
  let kind = match printer.uri.contains(&format!(".{IPPS}.")) {
    true => IPPS,
    false => IPP,
  };
  let server = ServerProxy::new(conn).await?;
  let item = (
    IF_UNSPEC,
    PROTO_UNSPEC,
    printer.name.clone(),
    kind.into(),
    "local".into(),
    0,
  );
  Ok(direct_uri(&resolve(&server, &item).await?))
}

/// Percent-encodes everything but unreserved URI characters.
fn encode(name: &str) -> String {
  name
    .bytes()
    .map(|b| match b {
      b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
        (b as char).to_string()
      }
      _ => format!("%{b:02X}"),
    })
    .collect()
}

fn decode(uri: &str) -> String {
  let bytes = uri.as_bytes();
  let mut out = Vec::with_capacity(bytes.len());
  let mut i = 0;
  while i < bytes.len() {
    let hex = bytes
      .get(i + 1..i + 3)
      .and_then(|h| std::str::from_utf8(h).ok());
    match (bytes[i], hex.and_then(|h| u8::from_str_radix(h, 16).ok())) {
      (b'%', Some(b)) => {
        out.push(b);
        i += 3;
      }
      (b, _) => {
        out.push(b);
        i += 1;
      }
    }
  }
  String::from_utf8_lossy(&out).into_owned()
}

// shortcut: a queue is matched to an announcement by its DNS-SD name or its
// address in the device URI, so a queue made with a host name is not
// recognized. Upgrade by comparing printer-uuid if that shows up.
/// Whether CUPS already has a queue for `printer`.
pub fn configured(printer: &NetworkPrinter, queues: &[Printer]) -> bool {
  queues.iter().any(|q| {
    let uri = decode(&q.device_uri);
    let host = uri
      .split_once("://")
      .map(|(_, rest)| rest.split(['/', '?']).next().unwrap_or(""))
      .unwrap_or("");
    let host = host.rsplit_once(':').map_or(host, |(h, port)| {
      // an IPv6 address has colons of its own
      if port.chars().all(|c| c.is_ascii_digit()) {
        h
      } else {
        host
      }
    });
    host.trim_matches(['[', ']']) == printer.address
      || uri.contains(&format!("//{}._ipp", printer.name))
      // added by us at an address that has changed since
      || q.name.eq_ignore_ascii_case(&base_name(printer))
  })
}

/// A CUPS queue name for `printer`: letters, digits, `-` and `_`, not taken yet.
pub fn queue_name(printer: &NetworkPrinter, queues: &[Printer]) -> String {
  let base = base_name(printer);
  let taken = |name: &str| queues.iter().any(|q| q.name.eq_ignore_ascii_case(name));
  let mut name = base.clone();
  let mut n = 2;
  while taken(&name) {
    name = format!("{base}_{n}");
    n += 1;
  }
  name
}

/// `printer`'s name as a queue name, before making it unique.
fn base_name(printer: &NetworkPrinter) -> String {
  let mut base = String::new();
  for c in printer.name.chars() {
    match c.is_ascii_alphanumeric() || c == '-' {
      true => base.push(c),
      false if !base.ends_with('_') => base.push('_'),
      false => {}
    }
  }
  let mut base = base.trim_matches('_').to_string();
  base.truncate(100);
  if base.is_empty() {
    base = "printer".into();
  }
  base
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::state::PrinterState;

  fn service(
    name: &str,
    kind: &str,
    interface: i32,
    txt: &[&str],
  ) -> ((String, String, i32, i32), Service) {
    let txt: Vec<_> = txt.iter().map(|t| t.as_bytes().to_vec()).collect();
    let service = Service::new(name, kind, &format!("10.0.0.{interface}"), 631, &txt);
    ((name.into(), kind.into(), interface, 0), service)
  }

  fn queue(name: &str, device_uri: &str) -> Printer {
    Printer {
      name: name.into(),
      info: String::new(),
      location: String::new(),
      device_uri: device_uri.into(),
      state: PrinterState::Idle,
      reasons: vec![],
      accepting: true,
      markers: vec![],
    }
  }

  #[test]
  fn prefers_plain_scanners() {
    let found: Found = [
      service("B", SCANS, 1, &[]),
      service("A", SCANS, 1, &[]),
      service("A", SCAN, 2, &[]),
      service("A", SCAN, 3, &[]),
      service("P", IPP, 1, &[]),
    ]
    .into_iter()
    .collect();
    let list = scanners(&found);
    assert_eq!(list.len(), 2);
    assert_eq!((list[0].name.as_str(), list[0].tls), ("A", false));
    assert_eq!((list[1].name.as_str(), list[1].tls), ("B", true));
  }

  #[test]
  fn printers_prefer_tls_and_skip_shared_queues() {
    let found: Found = [
      service("Brother HL", IPP, 1, &["ty=Brother HL-L2350DW"]),
      service("Brother HL", IPPS, 1, &[]),
      service("Office @ desk", IPP, 1, &["printer-type=0x801046"]),
      service("Scanner", SCAN, 1, &[]),
    ]
    .into_iter()
    .collect();
    let list = printers(&found);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].uri, "dnssd://Brother%20HL._ipps._tcp.local/");
    assert_eq!(list[0].direct_uri, "ipps://10.0.0.1:631/ipp/print");
    let v6: Found = [(
      ("P".into(), IPP.into(), 1, 1),
      Service::new("P", IPP, "fd00::7", 631, &[b"rp=/ipp/lp".to_vec()]),
    )]
    .into_iter()
    .collect();
    assert_eq!(printers(&v6)[0].direct_uri, "ipp://[fd00::7]:631/ipp/lp");
    assert_eq!(list[0].model, None, "the TLS record had no ty");
  }

  #[test]
  fn recognizes_configured_queues() {
    let printer = NetworkPrinter {
      name: "Brother HL".into(),
      model: None,
      address: "10.0.0.7".into(),
      uri: "dnssd://Brother%20HL._ipps._tcp.local/".into(),
      direct_uri: "ipps://10.0.0.7:631/ipp/print".into(),
    };
    let yes = [
      "dnssd://Brother%20HL._ipp._tcp.local/?uuid=e3248000",
      "ipp://10.0.0.7/ipp/print",
      "ipps://10.0.0.7:631/ipp/print",
    ];
    for uri in yes {
      assert!(configured(&printer, &[queue("q", uri)]), "{uri}");
    }
    // our own queue, its address changed since
    assert!(configured(
      &printer,
      &[queue("brother_hl", "ipp://10.0.0.99/ipp/print")]
    ));
    let no = [
      "ipp://10.0.0.70/ipp/print",
      "usb://Brother/HL",
      "dnssd://Other._ipp._tcp.local/",
      "",
    ];
    for uri in no {
      assert!(!configured(&printer, &[queue("q", uri)]), "{uri}");
    }
  }

  #[test]
  fn queue_names() {
    let printer = |name: &str| NetworkPrinter {
      name: name.into(),
      model: None,
      address: String::new(),
      uri: String::new(),
      direct_uri: String::new(),
    };
    assert_eq!(
      queue_name(&printer("Brother HL-L2350DW series"), &[]),
      "Brother_HL-L2350DW_series"
    );
    assert_eq!(queue_name(&printer("  ** "), &[]), "printer");
    let taken = [queue("office", ""), queue("Office_2", "")];
    assert_eq!(queue_name(&printer("Office"), &taken), "Office_3");
  }

  #[test]
  fn percent_coding() {
    assert_eq!(encode("Ä b/c"), "%C3%84%20b%2Fc");
    assert_eq!(decode("%C3%84%20b%2Fc"), "Ä b/c");
    assert_eq!(decode("100%"), "100%");
  }
}
