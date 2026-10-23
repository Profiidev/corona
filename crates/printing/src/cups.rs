//! CUPS operations on top of the IPP codec.

use std::{path::Path, time::Duration};

use anyhow::{Context, Result};

use crate::{
  ipp::{Attr, Group, IppError, Message, Value, op, send, send_within, tag},
  state::{Job, JobState, Marker, Printer, PrinterState},
};

pub const SOCKET: &str = "/run/cups/cups.sock";

/// how long a network printer gets to accept a connection
const PROBE: Duration = Duration::from_millis(1500);

/// Host and port of a network device URI, `None` for USB, dnssd and the like.
/// Not for `socket` or `lpd`: such printers take one connection at a time, and
/// a probe could block a job or even print an empty one.
pub fn device_addr(uri: &str) -> Option<(String, u16)> {
  let (scheme, rest) = uri.split_once("://")?;
  let default = match scheme {
    "ipp" | "ipps" => 631,
    "http" => 80,
    "https" => 443,
    _ => return None,
  };
  let authority = rest.split(['/', '?']).next()?.rsplit('@').next()?;
  let (host, port) = match authority.strip_prefix('[') {
    Some(v6) => {
      let (host, rest) = v6.split_once(']')?;
      (host, rest.strip_prefix(':'))
    }
    None => match authority.rsplit_once(':') {
      Some((host, port)) => (host, Some(port)),
      None => (authority, None),
    },
  };
  let port = match port {
    Some(port) => port.parse().ok()?,
    None => default,
  };
  (!host.is_empty()).then(|| (host.to_string(), port))
}

/// Marks the network printers that do not accept a connection, all at once.
pub async fn mark_offline(printers: &mut [Printer]) {
  let probes: Vec<_> = printers
    .iter()
    .map(|p| {
      // a printer that is printing is there, and busy with CUPS
      let addr = (p.state != PrinterState::Processing)
        .then(|| device_addr(&p.device_uri))
        .flatten();
      smol::spawn(async move {
        let Some(addr) = addr else { return false };
        let connect = async { smol::net::TcpStream::connect(addr).await.is_err() };
        futures_lite::future::or(connect, async {
          smol::Timer::after(PROBE).await;
          true
        })
        .await
      })
    })
    .collect();
  for (printer, probe) in printers.iter_mut().zip(probes) {
    printer.offline = probe.await;
  }
}

/// What the D-Bus notifier signals about. CUPS sends job events of every
/// printer to a subscription on the server URI.
const EVENTS: [&str; 7] = [
  "printer-added",
  "printer-deleted",
  "printer-modified",
  "printer-state-changed",
  "job-created",
  "job-completed",
  "job-state-changed",
];

pub const LEASE_SECONDS: i32 = 3600;

fn uri(value: &str) -> Value {
  Value::Str(tag::URI, value.into())
}

fn keyword(value: &str) -> Value {
  Value::Str(tag::KEYWORD, value.into())
}

fn name(value: &str) -> Value {
  Value::Str(tag::NAME, value.into())
}

fn user() -> Attr {
  let user = std::env::var("USER").unwrap_or_else(|_| "corona".into());
  Attr::new("requesting-user-name", name(&user))
}

fn printer_uri(printer: &str) -> Attr {
  Attr::new(
    "printer-uri",
    uri(&format!("ipp://localhost/printers/{printer}")),
  )
}

fn server_uri() -> Attr {
  Attr::new("printer-uri", uri("ipp://localhost/"))
}

/// cupsd could not reach the printer, e.g. it cannot resolve `.local` names.
pub fn device_error(e: &anyhow::Error) -> bool {
  matches!(
    e.downcast_ref::<IppError>(),
    Some(IppError::Status(0x0504, _))
  )
}

/// CUPS answers a list request with client-error-not-found when it is empty.
fn not_found(e: &anyhow::Error) -> bool {
  matches!(
    e.downcast_ref::<IppError>(),
    Some(IppError::Status(0x0406, _))
  )
}

pub async fn printers(sock: &Path) -> Result<Vec<Printer>> {
  let request = Message::request(
    op::CUPS_GET_PRINTERS,
    vec![
      user(),
      Attr::many(
        "requested-attributes",
        [
          "printer-name",
          "printer-info",
          "printer-location",
          "device-uri",
          "printer-state",
          "printer-state-reasons",
          "printer-is-accepting-jobs",
          "marker-names",
          "marker-levels",
          "marker-colors",
          "marker-types",
        ]
        .map(keyword),
      ),
    ],
  );
  let response = match send(sock, &request).await {
    Err(e) if not_found(&e) => return Ok(Vec::new()),
    response => response?,
  };
  let mut printers: Vec<_> = response.groups(tag::PRINTER).filter_map(printer).collect();
  printers.sort_by_key(|p| p.name.to_lowercase());
  Ok(printers)
}

fn printer(group: &Group) -> Option<Printer> {
  let text = |name: &str| {
    group
      .get(name)
      .and_then(Attr::str)
      .unwrap_or_default()
      .to_string()
  };
  let list = |name: &str| -> Vec<String> {
    group
      .get(name)
      .map(|a| a.strs().map(String::from).collect())
      .unwrap_or_default()
  };
  let levels: Vec<i32> = group
    .get("marker-levels")
    .map(|a| a.ints().collect())
    .unwrap_or_default();
  let (colors, kinds) = (list("marker-colors"), list("marker-types"));
  let markers = list("marker-names")
    .into_iter()
    .enumerate()
    .map(|(i, name)| Marker {
      name,
      // negative levels mean unknown
      level: levels
        .get(i)
        .and_then(|l| u8::try_from(*l).ok())
        .map(|l| l.min(100)),
      color: colors.get(i).and_then(|c| color(c)),
      kind: kinds.get(i).cloned().unwrap_or_default(),
    })
    .collect();
  let reasons = list("printer-state-reasons")
    .into_iter()
    .filter(|r| r != "none")
    .collect();
  Some(Printer {
    name: group.get("printer-name")?.str()?.to_string(),
    info: text("printer-info"),
    location: text("printer-location"),
    device_uri: text("device-uri"),
    state: match group.get("printer-state").and_then(Attr::int) {
      Some(4) => PrinterState::Processing,
      Some(5) => PrinterState::Stopped,
      _ => PrinterState::Idle,
    },
    reasons,
    accepting: group
      .get("printer-is-accepting-jobs")
      .and_then(Attr::bool)
      .unwrap_or(true),
    markers,
    offline: false,
  })
}

/// `#RRGGBB`; a multi-color marker like `#00FFFF#FF00FF` takes its first
fn color(value: &str) -> Option<u32> {
  let hex = value.strip_prefix('#')?.get(..6)?;
  u32::from_str_radix(hex, 16).ok()
}

/// Active jobs, then the most recent finished ones.
/// Unfinished jobs, then the last finished ones created at or after `since`
/// (unix seconds), so weeks-old history stays out.
pub async fn jobs(sock: &Path, since: i32) -> Result<Vec<Job>> {
  const RECENT: usize = 10;
  let mut active = get_jobs(sock, "not-completed").await?;
  let mut done = get_jobs(sock, "completed").await?;
  done.retain(|j| j.created.is_none_or(|t| t >= since));
  done.sort_by_key(|j| std::cmp::Reverse(j.id));
  done.truncate(RECENT);
  active.sort_by_key(|j| j.id);
  active.extend(done);
  Ok(active)
}

async fn get_jobs(sock: &Path, which: &str) -> Result<Vec<Job>> {
  let request = Message::request(
    op::GET_JOBS,
    vec![
      server_uri(),
      user(),
      Attr::new("which-jobs", keyword(which)),
      Attr::many(
        "requested-attributes",
        [
          "job-id",
          "job-name",
          "job-state",
          "job-printer-uri",
          "job-impressions",
          "job-impressions-completed",
          "job-media-sheets-completed",
          "time-at-creation",
        ]
        .map(keyword),
      ),
    ],
  );
  let response = match send(sock, &request).await {
    Err(e) if not_found(&e) => return Ok(Vec::new()),
    response => response?,
  };
  Ok(response.groups(tag::JOB).filter_map(job).collect())
}

fn job(group: &Group) -> Option<Job> {
  let int = |name: &str| group.get(name).and_then(Attr::int);
  Some(Job {
    id: int("job-id")?,
    name: group
      .get("job-name")
      .and_then(Attr::str)
      .unwrap_or_default()
      .to_string(),
    printer: group
      .get("job-printer-uri")
      .and_then(Attr::str)
      .and_then(|u| u.rsplit('/').next())
      .unwrap_or_default()
      .to_string(),
    state: JobState::from_ipp(int("job-state")?),
    pages_done: int("job-impressions-completed").or(int("job-media-sheets-completed")),
    pages_total: int("job-impressions").filter(|n| *n > 0),
    created: int("time-at-creation"),
  })
}

pub async fn cancel_job(sock: &Path, id: i32) -> Result<()> {
  let request = Message::request(
    op::CANCEL_JOB,
    vec![server_uri(), Attr::new("job-id", Value::Int(id)), user()],
  );
  send(sock, &request).await.map(drop)
}

pub async fn set_paused(sock: &Path, printer: &str, paused: bool) -> Result<()> {
  let op = if paused {
    op::PAUSE_PRINTER
  } else {
    op::RESUME_PRINTER
  };
  let request = Message::request(op, vec![printer_uri(printer), user()]);
  send(sock, &request).await.map(drop)
}

/// Returns the new job's id.
pub async fn print(sock: &Path, printer: &str, title: &str, data: Vec<u8>) -> Result<i32> {
  let mut request = Message::request(
    op::PRINT_JOB,
    vec![
      printer_uri(printer),
      user(),
      Attr::new("job-name", name(title)),
      Attr::new(
        "document-format",
        Value::Str(tag::MIME, "application/octet-stream".into()),
      ),
    ],
  );
  request.data = data;
  let response = send(sock, &request).await?;
  response
    .groups(tag::JOB)
    .find_map(|g| g.get("job-id")?.int())
    .context("CUPS returned no job-id")
}

/// The PPD name that has CUPS ask the printer itself (IPP Everywhere).
pub const DRIVERLESS: &str = "everywhere";

/// Makes a queue for a printer with the driver `ppd`, ready for jobs.
pub async fn add_printer(
  sock: &Path,
  queue: &str,
  device_uri: &str,
  ppd: &str,
  info: &str,
) -> Result<()> {
  let mut request = Message::request(
    op::CUPS_ADD_MODIFY_PRINTER,
    vec![printer_uri(queue), user(), Attr::new("ppd-name", name(ppd))],
  );
  request.groups.push(Group::new(
    tag::PRINTER,
    vec![
      Attr::new("device-uri", uri(device_uri)),
      Attr::new("printer-info", Value::Str(tag::TEXT, info.into())),
      Attr::new("printer-is-accepting-jobs", Value::Bool(true)),
      Attr::new("printer-state", Value::Enum(3)),
    ],
  ));
  // CUPS asks the printer before it answers, which can take a while
  send_within(sock, &request, Duration::from_secs(120))
    .await
    .map(drop)
}

/// The model number in `model`, letters and digits only: `CLX-3180` in
/// `Samsung CLX-3180 Series`.
fn model_key(model: &str) -> Option<String> {
  model
    .split_whitespace()
    .filter(|word| word.chars().any(|c| c.is_ascii_digit()))
    .map(|word| {
      let key: String = word.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
      key.to_ascii_lowercase()
    })
    .max_by_key(String::len)
}

/// An installed driver for `model`, the closest match first.
pub async fn find_driver(sock: &Path, model: &str) -> Result<Option<String>> {
  let Some(key) = model_key(model) else {
    return Ok(None);
  };
  // the make too, a bare model number like `1020` is in other makes' drivers
  let make: String = model
    .split_whitespace()
    .next()
    .unwrap_or_default()
    .chars()
    .filter(|c| c.is_ascii_alphanumeric())
    .collect::<String>()
    .to_ascii_lowercase();
  let request = Message::request(
    op::CUPS_GET_PPDS,
    vec![
      user(),
      Attr::many(
        "requested-attributes",
        ["ppd-name", "ppd-make-and-model"].map(keyword),
      ),
    ],
  );
  // cups-driverd reads every installed driver, slow on a cold cache
  let response = match send_within(sock, &request, Duration::from_secs(120)).await {
    Err(e) if not_found(&e) => return Ok(None),
    response => response?,
  };
  let drivers = response.groups(tag::PRINTER).filter_map(|g| {
    let model = g.get("ppd-make-and-model")?.str()?;
    let flat: String = model
      .chars()
      .filter(|c| c.is_ascii_alphanumeric())
      .collect();
    let flat = flat.to_ascii_lowercase();
    (flat.contains(&key) && flat.contains(&make))
      .then(|| Some((model.len(), g.get("ppd-name")?.str()?.to_string())))?
  });
  Ok(drivers.min_by_key(|(len, _)| *len).map(|(_, ppd)| ppd))
}

/// Asks CUPS to send its events as `org.cups.cupsd.Notifier` signals.
/// Returns the subscription id.
pub async fn subscribe(sock: &Path) -> Result<i32> {
  let mut request = Message::request(op::CREATE_PRINTER_SUBSCRIPTION, vec![server_uri(), user()]);
  request.groups.push(Group::new(
    tag::SUBSCRIPTION,
    vec![
      Attr::new("notify-recipient-uri", uri("dbus://")),
      Attr::many("notify-events", EVENTS.map(keyword)),
      Attr::new("notify-lease-duration", Value::Int(LEASE_SECONDS)),
    ],
  ));
  let response = send(sock, &request).await?;
  response
    .groups(tag::SUBSCRIPTION)
    .find_map(|g| g.get("notify-subscription-id")?.int())
    .context("CUPS returned no notify-subscription-id")
}

pub async fn renew(sock: &Path, id: i32) -> Result<()> {
  let mut request = Message::request(
    op::RENEW_SUBSCRIPTION,
    vec![
      server_uri(),
      Attr::new("notify-subscription-id", Value::Int(id)),
      user(),
    ],
  );
  request.groups.push(Group::new(
    tag::SUBSCRIPTION,
    vec![Attr::new(
      "notify-lease-duration",
      Value::Int(LEASE_SECONDS),
    )],
  ));
  send(sock, &request).await.map(drop)
}

pub async fn unsubscribe(sock: &Path, id: i32) -> Result<()> {
  let request = Message::request(
    op::CANCEL_SUBSCRIPTION,
    vec![
      server_uri(),
      Attr::new("notify-subscription-id", Value::Int(id)),
      user(),
    ],
  );
  send(sock, &request).await.map(drop)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn reads_printers() {
    let group = Group::new(
      tag::PRINTER,
      vec![
        Attr::new("printer-name", name("Office")),
        Attr::new("printer-state", Value::Enum(5)),
        Attr::many(
          "printer-state-reasons",
          [keyword("paused"), keyword("toner-low-warning")],
        ),
        Attr::new("printer-is-accepting-jobs", Value::Bool(false)),
        Attr::many(
          "marker-names",
          ["Black", "Cyan"].map(|s| Value::Str(tag::NAME, s.into())),
        ),
        Attr::many("marker-levels", [Value::Int(42), Value::Int(-1)]),
        Attr::many(
          "marker-colors",
          ["#000000", "#00FFFF#FF00FF"].map(|s| Value::Str(tag::NAME, s.into())),
        ),
      ],
    );
    let p = printer(&group).unwrap();
    assert_eq!(p.state, PrinterState::Stopped);
    assert!(!p.accepting);
    assert_eq!(p.reasons, ["paused", "toner-low-warning"]);
    assert_eq!(p.markers[0].level, Some(42));
    assert_eq!(p.markers[1].level, None);
    assert_eq!(p.markers[1].color, Some(0x00FFFF));
    assert_eq!(p.markers[1].kind, "");
  }

  #[test]
  fn device_addresses() {
    let addr = |uri| device_addr(uri).map(|(h, p)| format!("{h} {p}"));
    assert_eq!(
      addr("ipp://192.168.178.119:631/ipp/print?version=1.0").as_deref(),
      Some("192.168.178.119 631")
    );
    assert_eq!(
      addr("ipps://Brother.local/ipp/print").as_deref(),
      Some("Brother.local 631")
    );
    assert_eq!(addr("socket://10.0.0.7?waiteof=false"), None);
    assert_eq!(
      addr("ipp://user@[fe80::1]:8631/").as_deref(),
      Some("fe80::1 8631")
    );
    assert_eq!(addr("usb://Samsung/CLX-3180"), None);
    assert_eq!(addr("dnssd://Office._ipp._tcp.local/"), None);
  }

  #[test]
  fn printer_without_name_is_skipped() {
    let group = Group::new(
      tag::PRINTER,
      vec![Attr::new("printer-state", Value::Enum(3))],
    );
    assert!(printer(&group).is_none());
    assert_eq!(color("#ff8000"), Some(0xFF8000));
    assert_eq!(color("none"), None);
  }

  #[test]
  fn model_keys() {
    assert_eq!(
      model_key("Samsung CLX-3180 Series").as_deref(),
      Some("clx3180")
    );
    assert_eq!(
      model_key("Brother MFC-L2700DW series").as_deref(),
      Some("mfcl2700dw")
    );
    assert_eq!(model_key("Office printer"), None);
  }

  #[test]
  fn reads_jobs() {
    let group = Group::new(
      tag::JOB,
      vec![
        Attr::new("job-id", Value::Int(12)),
        Attr::new("job-name", name("report.pdf")),
        Attr::new("job-state", Value::Enum(5)),
        Attr::new("job-printer-uri", uri("ipp://localhost/printers/Office")),
        Attr::new("job-media-sheets-completed", Value::Int(2)),
        Attr::new("job-impressions", Value::Int(0)),
      ],
    );
    let j = job(&group).unwrap();
    assert_eq!(j.printer, "Office");
    assert_eq!(j.state, JobState::Processing);
    assert_eq!((j.pages_done, j.pages_total), (Some(2), None));
  }
}
