//! A fake cupsd on a Unix socket, speaking the same IPP codec.

use std::{
  io::{Read, Write},
  os::unix::net::UnixListener,
  path::PathBuf,
  sync::{Arc, Mutex},
};

use tempfile::TempDir;

use crate::ipp::{Attr, Group, Message, Value, op, tag};

#[derive(Default)]
pub struct State {
  pub printers: Vec<Group>,
  pub jobs: Vec<Group>,
  /// answer admin operations with client-error-not-authorized
  pub deny_admin: bool,
  /// answer every request with HTTP 500, like a broken cupsd
  pub broken: bool,
  /// answer admin operations with HTTP 401 and keep the connection open,
  /// like cupsd, unless PeerCred is sent and `peercred_admin` is set
  pub http_auth: bool,
  pub peercred_admin: bool,
  /// fail queues with a `dnssd://` device URI, like a cupsd without nss-mdns
  pub unresolvable_dnssd: bool,
  /// refuse IPP Everywhere like an old printer that only claims IPP
  pub not_driverless: bool,
  /// drivers as (ppd-name, ppd-make-and-model)
  pub ppds: Vec<(String, String)>,
  /// how many requests came with `Authorization: PeerCred`
  pub peercred_tries: usize,
  pub requests: Vec<Message>,
}

pub struct MockCups {
  pub sock: PathBuf,
  pub state: Arc<Mutex<State>>,
  _dir: TempDir,
}

/// The body of a whole HTTP request, `None` while it is still arriving.
fn request_body(raw: &[u8]) -> Option<&[u8]> {
  let end = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
  let head = String::from_utf8_lossy(&raw[..end]);
  let len: usize = head.split("\r\n").find_map(|l| {
    let (k, v) = l.split_once(':')?;
    k.eq_ignore_ascii_case("content-length")
      .then(|| v.trim().parse().ok())?
  })?;
  raw.get(end + 4..end + 4 + len)
}

pub fn printer(name: &str, state: i32) -> Group {
  Group::new(
    tag::PRINTER,
    vec![
      Attr::new("printer-name", Value::Str(tag::NAME, name.into())),
      Attr::new("printer-state", Value::Enum(state)),
      Attr::new("printer-is-accepting-jobs", Value::Bool(true)),
      Attr::new(
        "device-uri",
        Value::Str(tag::URI, format!("usb://Maker/{name}")),
      ),
      Attr::new("marker-names", Value::Str(tag::NAME, "Black".into())),
      Attr::new("marker-levels", Value::Int(80)),
    ],
  )
}

pub fn job(id: i32, printer: &str, state: i32) -> Group {
  Group::new(
    tag::JOB,
    vec![
      Attr::new("job-id", Value::Int(id)),
      Attr::new("job-name", Value::Str(tag::NAME, format!("job {id}"))),
      Attr::new("job-state", Value::Enum(state)),
      Attr::new(
        "job-printer-uri",
        Value::Str(tag::URI, format!("ipp://localhost/printers/{printer}")),
      ),
    ],
  )
}

fn set(group: &mut Group, name: &str, value: Value) {
  match group.attrs.iter_mut().find(|a| a.name == name) {
    Some(attr) => attr.values = vec![value],
    None => group.attrs.push(Attr::new(name, value)),
  }
}

impl MockCups {
  pub fn start() -> Self {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("cups.sock");
    let listener = UnixListener::bind(&sock).unwrap();
    let state = Arc::new(Mutex::new(State::default()));
    let shared = state.clone();
    std::thread::spawn(move || {
      // connections cupsd would leave open after a 401
      let mut held = Vec::new();
      for stream in listener.incoming() {
        let Ok(mut stream) = stream else { break };
        let mut raw = Vec::new();
        let mut buf = [0; 4096];
        // read until the whole body announced by Content-Length is in
        loop {
          let n = stream.read(&mut buf).unwrap_or(0);
          raw.extend(&buf[..n]);
          if n == 0 || request_body(&raw).is_some() {
            break;
          }
        }
        let peercred = String::from_utf8_lossy(&raw).contains("\r\nAuthorization: PeerCred ");
        let (status, body) = {
          let mut state = shared.lock().unwrap();
          state.peercred_tries += peercred as usize;
          let admin = request_body(&raw)
            .and_then(|b| Message::parse(b).ok())
            .is_some_and(|r| {
              [
                op::PAUSE_PRINTER,
                op::RESUME_PRINTER,
                op::CUPS_ADD_MODIFY_PRINTER,
              ]
              .contains(&r.code)
            });
          match request_body(&raw) {
            _ if state.broken => (500, Vec::new()),
            _ if admin && state.http_auth && !(peercred && state.peercred_admin) => {
              (401, b"<HTML>Unauthorized</HTML>".to_vec())
            }
            Some(body) => match Message::parse(body) {
              Ok(request) => {
                let response = answer(&mut state, &request);
                state.requests.push(request);
                (200, response.encode())
              }
              Err(_) => (400, Vec::new()),
            },
            None => (400, Vec::new()),
          }
        };
        let head = format!(
          "HTTP/1.1 {status} X\r\nContent-Type: application/ipp\r\nContent-Length: {}\r\n\r\n",
          body.len()
        );
        stream.write_all(head.as_bytes()).ok();
        stream.write_all(&body).ok();
        if status == 401 {
          held.push(stream);
        }
      }
    });
    Self {
      sock,
      state,
      _dir: dir,
    }
  }

  pub fn ops(&self) -> Vec<u16> {
    self
      .state
      .lock()
      .unwrap()
      .requests
      .iter()
      .map(|r| r.code)
      .collect()
  }
}

fn answer(state: &mut State, request: &Message) -> Message {
  let mut response = Message::request(0, Vec::new());
  response.request_id = request.request_id;
  let int = |name: &str| request.operation(name).and_then(Attr::int);
  match request.code {
    // like cupsd, an empty list is client-error-not-found
    op::CUPS_GET_PRINTERS if state.printers.is_empty() => response.code = 0x0406,
    op::CUPS_GET_PRINTERS => response.groups.extend(state.printers.clone()),
    op::GET_JOBS => {
      let completed = request.operation("which-jobs").and_then(Attr::str) == Some("completed");
      let jobs = state.jobs.iter().filter(|j| {
        let done = j.get("job-state").and_then(Attr::int).unwrap_or(9) >= 7;
        done == completed
      });
      response.groups.extend(jobs.cloned());
    }
    op::CANCEL_JOB => {
      let id = int("job-id");
      for job in &mut state.jobs {
        if job.get("job-id").and_then(Attr::int) == id {
          set(job, "job-state", Value::Enum(7));
        }
      }
    }
    op::PAUSE_PRINTER | op::RESUME_PRINTER | op::CUPS_ADD_MODIFY_PRINTER if state.deny_admin => {
      response.code = 0x0403
    }
    op::CUPS_ADD_MODIFY_PRINTER
      if state.unresolvable_dnssd
        && request.groups(tag::PRINTER).any(|g| {
          let uri = g.get("device-uri").and_then(Attr::str);
          uri.is_some_and(|u| u.starts_with("dnssd:"))
        }) =>
    {
      response.code = 0x0504;
      response.groups[0].attrs.push(Attr::new(
        "status-message",
        Value::Str(tag::TEXT, "Unable to connect to printer.local:631".into()),
      ));
    }
    op::CUPS_ADD_MODIFY_PRINTER
      if state.not_driverless
        && request.operation("ppd-name").and_then(Attr::str) == Some("everywhere") =>
    {
      response.code = 0x0504;
      response.groups[0].attrs.push(Attr::new(
        "status-message",
        Value::Str(tag::TEXT, "Printer returned invalid data".into()),
      ));
    }
    op::CUPS_GET_PPDS => response
      .groups
      .extend(state.ppds.iter().map(|(name, model)| {
        Group::new(
          tag::PRINTER,
          vec![
            Attr::new("ppd-name", Value::Str(tag::NAME, name.clone())),
            Attr::new("ppd-make-and-model", Value::Str(tag::TEXT, model.clone())),
          ],
        )
      })),
    op::CUPS_ADD_MODIFY_PRINTER => {
      let uri = request.operation("printer-uri").and_then(Attr::str);
      let queue = uri.and_then(|u| u.rsplit('/').next()).unwrap_or("");
      let mut added = printer(queue, 3);
      let device = request
        .groups(tag::PRINTER)
        .find_map(|g| g.get("device-uri")?.str().map(String::from))
        .unwrap_or_default();
      set(&mut added, "device-uri", Value::Str(tag::URI, device));
      state.printers.push(added);
    }
    op::PAUSE_PRINTER | op::RESUME_PRINTER => {
      let uri = request.operation("printer-uri").and_then(Attr::str);
      let next = if request.code == op::PAUSE_PRINTER {
        5
      } else {
        3
      };
      for printer in &mut state.printers {
        let name = printer
          .get("printer-name")
          .and_then(Attr::str)
          .unwrap_or("");
        if uri.is_some_and(|u| u.ends_with(&format!("/{name}"))) {
          set(printer, "printer-state", Value::Enum(next));
        }
      }
    }
    op::PRINT_JOB => {
      let id = 100 + state.jobs.len() as i32;
      let printer = request
        .operation("printer-uri")
        .and_then(Attr::str)
        .and_then(|u| u.rsplit('/').next())
        .unwrap_or("")
        .to_string();
      state.jobs.push(job(id, &printer, 3));
      response.groups.push(Group::new(
        tag::JOB,
        vec![Attr::new("job-id", Value::Int(id))],
      ));
    }
    op::CREATE_PRINTER_SUBSCRIPTION => response.groups.push(Group::new(
      tag::SUBSCRIPTION,
      vec![Attr::new("notify-subscription-id", Value::Int(7))],
    )),
    _ => {}
  }
  response
}
