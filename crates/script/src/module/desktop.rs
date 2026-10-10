//! `corona/desktop`: the file picker, the user's default apps and OAuth
//! redirects to a loopback port.

use std::{
  cell::{Cell, RefCell},
  collections::HashMap,
  fs,
  net::Ipv4Addr,
  os::unix::fs::PermissionsExt as _,
  path::{Path, PathBuf},
  rc::Rc,
  sync::Arc,
  time::Duration,
};

use anyhow::{Context as _, Result, anyhow, ensure};
use corona_macros::named;
use futures::stream::{FuturesUnordered, StreamExt as _};
use futures_lite::{AsyncReadExt as _, AsyncWriteExt as _, future};
use gpui_kit::{App, PathPromptOptions, Subscription};
use gpui_shell::{Capabilities, HostModule};
use serde::{Deserialize, Serialize};
use smol::net::{TcpListener, TcpStream};
use ts_rs::TS;

use crate::{host_fn::Module, module::Subscribe};

#[derive(Default, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
struct PickFiles {
  multiple: Option<bool>,
  /// Pick directories instead of files.
  directory: Option<bool>,
  /// The prompt on the dialog.
  accept_label: Option<String>,
}

/// `path` as it is opened: an existing absolute regular file, neither a
/// launcher nor an executable, which would run instead of open.
fn openable(path: &Path) -> Result<PathBuf> {
  ensure!(path.is_absolute(), "`{}` is not absolute", path.display());
  let path = fs::canonicalize(path).with_context(|| path.display().to_string())?;
  ensure!(
    !path
      .extension()
      .is_some_and(|ext| ext.eq_ignore_ascii_case("desktop")),
    "`{}` is a launcher",
    path.display()
  );
  let meta = fs::metadata(&path)?;
  ensure!(meta.is_file(), "`{}` is not a file", path.display());
  ensure!(
    meta.permissions().mode() & 0o111 == 0,
    "`{}` is executable",
    path.display()
  );
  Ok(path)
}

/// Schemes `openUri` opens; others hand the URI to whatever app claims them.
const SCHEMES: [&str; 3] = ["http", "https", "mailto"];

/// A URI with a web or mail scheme; files go through `openPath`, which checks them.
fn uri_ok(uri: &str) -> Result<()> {
  let scheme = uri
    .split_once(':')
    .map(|(scheme, _)| scheme)
    .with_context(|| format!("`{uri}` has no scheme"))?;
  ensure!(
    SCHEMES.iter().any(|s| s.eq_ignore_ascii_case(scheme)),
    "`{scheme}:` cannot be opened, only {SCHEMES:?}; open files with openPath"
  );
  Ok(())
}

#[derive(Default, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
struct ListenRedirect {
  /// How long `nextRedirect` waits, 5 minutes by default, 10 at most.
  timeout_ms: Option<u32>,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
struct Listening {
  id: u32,
  port: u16,
  /// `http://127.0.0.1:<port>/`, any path on it is taken.
  redirect_uri: String,
}

/// The request the browser was redirected with.
#[derive(Debug, PartialEq, Serialize, TS)]
struct Redirect {
  /// Percent-decoded, without the query.
  path: String,
  /// Percent-decoded, the last value of a repeated key wins.
  query: HashMap<String, String>,
}

const TIMEOUT: Duration = Duration::from_secs(5 * 60);
const MAX_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// Ports a load may hold open at once, waiting or not
const MAX_LISTENERS: usize = 2;
/// Longer request heads are not a redirect
const MAX_HEAD: usize = 16 * 1024;

fn decode(s: &str) -> String {
  let (bytes, mut out, mut i) = (s.as_bytes(), Vec::new(), 0);
  while i < bytes.len() {
    let hex = bytes
      .get(i + 1..i + 3)
      .filter(|h| bytes[i] == b'%' && h.iter().all(u8::is_ascii_hexdigit));
    match hex {
      Some(h) => {
        // two hex digits always fit
        out.push(u8::from_str_radix(std::str::from_utf8(h).unwrap(), 16).unwrap());
        i += 3;
      }
      None => {
        out.push(bytes[i]);
        i += 1;
      }
    }
  }
  String::from_utf8_lossy(&out).into_owned()
}

/// `target` of `GET <target> HTTP/1.1`
fn parse_target(target: &str) -> Redirect {
  let (path, query) = target.split_once('?').unwrap_or((target, ""));
  let query = query
    .split('&')
    .filter(|pair| !pair.is_empty())
    .map(|pair| {
      let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
      (
        decode(&key.replace('+', " ")),
        decode(&value.replace('+', " ")),
      )
    })
    .collect();
  Redirect {
    path: decode(path),
    query,
  }
}

/// The target of a GET on `stream`, with its head read so the reply is not
/// reset; `None` for anything else, like a browser's idle preconnect
async fn request(mut stream: TcpStream) -> Option<(TcpStream, String)> {
  let (mut head, mut chunk) = (Vec::new(), [0; 1024]);
  while !head.windows(4).any(|w| w == b"\r\n\r\n") {
    let n = stream.read(&mut chunk).await.ok()?;
    if n == 0 || head.len() > MAX_HEAD {
      return None;
    }
    head.extend_from_slice(&chunk[..n]);
  }
  let line = head.split(|&b| b == b'\r').next()?;
  let target = std::str::from_utf8(line).ok()?.strip_prefix("GET ")?;
  Some((stream, target.split(' ').next()?.to_string()))
}

/// Answers the first GET on `listener`, handling connections side by side
async fn redirect(listener: TcpListener) -> Result<Redirect> {
  enum Event {
    Accepted(std::io::Result<TcpStream>),
    Read(Option<(TcpStream, String)>),
  }
  let mut pending = FuturesUnordered::new();
  loop {
    let accept = async { Event::Accepted(listener.accept().await.map(|(s, _)| s)) };
    let read = async {
      match pending.next().await {
        Some(read) => Event::Read(read),
        None => future::pending().await,
      }
    };
    let event = future::or(accept, read).await;
    match event {
      Event::Accepted(stream) => pending.push(request(stream?)),
      Event::Read(Some((mut stream, target))) => {
        let page = "<!doctype html><title>corona</title><p>You can close this tab.</p>";
        let reply = format!(
          "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
           Content-Length: {}\r\nConnection: close\r\n\r\n{page}",
          page.len()
        );
        stream.write_all(reply.as_bytes()).await.ok();
        stream.flush().await.ok();
        return Ok(parse_target(&target));
      }
      Event::Read(None) => {}
    }
  }
}

/// `capabilities` is the plugin's grant, which a picked path is added to
pub fn module(capabilities: Capabilities, subs: &mut Vec<Subscribe>) -> HostModule {
  // each listener holds a clone of `live`, waiting or not
  type Listener = (TcpListener, Duration, Arc<()>);
  let listeners = Rc::new(RefCell::new(HashMap::<u32, Listener>::new()));
  let live = Arc::new(());
  let next_id = Rc::new(Cell::new(0));
  // never sent, dropped on cleanup to stop a waiting `nextRedirect`, whose
  // future gpui-shell never drops
  let (stop_tx, stop_rx) = flume::bounded::<()>(0);
  let dropped = listeners.clone();
  subs.push(Subscribe::Cleanup(Subscription::new(move || {
    drop(stop_tx);
    dropped.borrow_mut().clear();
  })));
  let taken = listeners.clone();
  let readable = capabilities.clone();

  Module::new("corona/desktop")
    .func(named!(
      "pickFiles",
      /// Asks the user for files, null when they cancel. What they pick is
      /// readable by this plugin until it restarts, a directory with
      /// everything in it.
      move |cx: &mut App, options: Option<PickFiles>| {
        let options = options.unwrap_or_default();
        let directory = options.directory.unwrap_or(false);
        let paths = cx.prompt_for_paths(PathPromptOptions {
          files: !directory,
          directories: directory,
          multiple: options.multiple.unwrap_or(false),
          prompt: options.accept_label.map(Into::into),
        });
        let capabilities = capabilities.clone();
        async move {
          let paths = paths.await??;
          anyhow::Ok(paths.map(|paths| {
            paths
              .into_iter()
              .map(|path| {
                let shown = path.to_string_lossy().into_owned();
                capabilities.grant_read(path);
                shown
              })
              .collect::<Vec<_>>()
          }))
        }
      }
    ))
    .func(named!(
      "openPath",
      /// Opens a file the plugin may read in the user's default app.
      move |cx: &mut App, path: String| -> Result<()> {
        let path = openable(Path::new(&path))?;
        ensure!(readable.may_read(&path), "`{}` is not readable", path.display());
        cx.open_with_system(&path);
        Ok(())
      }
    ))
    .func(named!(
      "openUri",
      /// Opens a URI, e.g. `https:` or `mailto:`, in the user's default app.
      |cx: &mut App, uri: String| -> Result<()> {
        uri_ok(&uri)?;
        cx.open_url(&uri);
        Ok(())
      }
    ))
    .func(named!(
      "listenRedirect",
      /// Listens on 127.0.0.1 for one OAuth redirect, for a login opened with
      /// `openUri`, on a free port. Wait for it with `nextRedirect(id)`.
      move |options: Option<ListenRedirect>| -> Result<Listening> {
        let options = options.unwrap_or_default();
        ensure!(
          Arc::strong_count(&live) <= MAX_LISTENERS,
          "at most {MAX_LISTENERS} redirect listeners at once"
        );
        let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let port = listener.local_addr()?.port();
        let id = next_id.get() + 1;
        next_id.set(id);
        let timeout = options
          .timeout_ms
          .map_or(TIMEOUT, |ms| Duration::from_millis(ms.into()))
          .min(MAX_TIMEOUT);
        let listener = (TcpListener::try_from(listener)?, timeout, live.clone());
        (listeners.borrow_mut()).insert(id, listener);
        Ok(Listening {
          id,
          port,
          redirect_uri: format!("http://127.0.0.1:{port}/"),
        })
      }
    ))
    .func(named!(
      "nextRedirect",
      /// The redirect `listenRedirect` waits for; the browser gets a page
      /// saying the tab can be closed. The port closes after it or the
      /// timeout.
      move |id: u32| {
        let listener = taken.borrow_mut().remove(&id);
        let stop = stop_rx.clone();
        async move {
          let (listener, timeout, _live) = listener.context("no such listener, or it was used")?;
          let timed_out = async {
            smol::Timer::after(timeout).await;
            Err(anyhow!("no redirect within {timeout:?}"))
          };
          let stopped = async {
            stop.recv_async().await.ok();
            Err(anyhow!("plugin stopped"))
          };
          future::or(redirect(listener), future::or(timed_out, stopped)).await
        }
      }
    ))
    .into()
}

#[cfg(test)]
mod tests {
  use corona_utils::test_bus::wait_until;
  use gpui_kit::{self as gpui, TestAppContext};

  use super::*;
  use crate::module::harness;

  #[gpui::test]
  fn refused_uris_never_reach_the_platform(cx: &mut TestAppContext) {
    let body = r#"report([m.openUri("file:///etc/passwd"), m.openPath("a.txt"), m.openPath("/etc/hostname")]);"#;
    let (view, cx) = harness::view(cx, body, |_, subs, _| module(Capabilities::new(), subs));
    let reports = view.last();
    assert!(reports[0]["message"].as_str().unwrap().contains("openPath"));
    assert!(
      reports[1]["message"]
        .as_str()
        .unwrap()
        .contains("not absolute")
    );
    // ungranted
    assert!(
      reports[2]["message"]
        .as_str()
        .unwrap()
        .contains("not readable")
    );
    assert_eq!(cx.opened_url(), None);
  }

  #[gpui::test]
  fn opens_uris(cx: &mut TestAppContext) {
    let (view, cx) = harness::view(cx, r#"report(m.openUri("mailto:a@b.c"));"#, |_, subs, _| {
      module(Capabilities::new(), subs)
    });
    assert_eq!(view.last(), serde_json::Value::Null);
    assert_eq!(cx.opened_url().as_deref(), Some("mailto:a@b.c"));
  }

  #[gpui::test]
  fn takes_one_redirect(cx: &mut TestAppContext) {
    use std::io::{Read as _, Write as _};

    cx.executor().allow_parking();
    let body = r#"if (!globalThis.started) {
      globalThis.started = true;
      const l = m.listenRedirect();
      // the port is always a free one
      const t = m.listenRedirect({ timeoutMs: 1, port: 1 });
      report({ ...l, capped: m.listenRedirect(), freePort: t.port !== 1 });
      m.nextRedirect(l.id).then(report);
      m.nextRedirect(l.id).then(report);
      m.nextRedirect(t.id).then(report);
    }"#;
    let (view, cx) = harness::view(cx, body, |_, subs, _| module(Capabilities::new(), subs));
    wait_until(cx, |_| view.reports.borrow().len() == 3);
    let listening = view.reports.borrow()[0].clone();
    let capped = listening["capped"]["message"].as_str().unwrap();
    assert!(capped.contains("at most"), "{capped}");
    assert_eq!(listening["freePort"], true);
    let port = listening["port"].as_u64().unwrap();
    assert_eq!(
      listening["redirectUri"],
      format!("http://127.0.0.1:{port}/")
    );
    let errors: Vec<String> = view.reports.borrow()[1..]
      .iter()
      .map(|e| e["message"].as_str().unwrap().to_string())
      .collect();
    assert!(errors.iter().any(|e| e.contains("was used")), "{errors:?}");
    assert!(
      errors.iter().any(|e| e.contains("no redirect")),
      "{errors:?}"
    );

    // an idle preconnect does not hold the redirect up
    let _idle = std::net::TcpStream::connect(("127.0.0.1", port as u16)).unwrap();
    let mut browser = std::net::TcpStream::connect(("127.0.0.1", port as u16)).unwrap();
    browser
      .write_all(b"GET /cb?code=x&state=y%20z+w&flag HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
      .unwrap();
    wait_until(cx, |_| view.reports.borrow().len() == 4);
    assert_eq!(
      view.last(),
      serde_json::json!({
        "path": "/cb",
        "query": { "code": "x", "state": "y z w", "flag": "" },
      })
    );
    let mut reply = String::new();
    browser.read_to_string(&mut reply).unwrap();
    assert!(reply.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(reply.ends_with("You can close this tab.</p>"));
    // the port is closed after the one redirect
    assert!(std::net::TcpStream::connect(("127.0.0.1", port as u16)).is_err());
  }

  #[test]
  fn decodes_targets() {
    assert_eq!(decode("a%2Fb%zz%4"), "a/b%zz%4");
    assert_eq!(decode("%C3%BC+"), "ü+");
    assert_eq!(decode("%ff"), "\u{fffd}");
    let redirect = parse_target("/a%20b?x=1&x=2&&k%3D=v%3D");
    assert_eq!(redirect.path, "/a b");
    assert_eq!(
      redirect.query,
      HashMap::from([("x".into(), "2".into()), ("k=".into(), "v=".into())])
    );
    assert_eq!(parse_target("/").query, HashMap::new());
  }

  #[test]
  fn openable_paths() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.txt");
    fs::write(&file, "").unwrap();
    assert!(openable(&file).is_ok());
    assert!(openable(dir.path()).is_err());

    assert!(openable(Path::new("a.txt")).is_err());
    assert!(openable(&dir.path().join("missing")).is_err());

    let launcher = dir.path().join("a.DESKTOP");
    fs::write(&launcher, "").unwrap();
    assert!(openable(&launcher).is_err());
    // nor through a link
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&launcher, &link).unwrap();
    assert!(openable(&link).is_err());

    let script = dir.path().join("run.sh");
    fs::write(&script, "").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(openable(&script).is_err());
  }

  #[test]
  fn uris() {
    for ok in ["https://example.com", "HTTP://x", "mailto:a@b.c"] {
      assert!(uri_ok(ok).is_ok(), "{ok}");
    }
    for bad in [
      "file:///etc/passwd",
      "FILE:/x",
      "kdeconnect://x",
      "javascript:alert(1)",
      "smb://host/share",
      "example.com",
      ":x",
      "1a:x",
      "a b:x",
      "",
    ] {
      assert!(uri_ok(bad).is_err(), "{bad}");
    }
  }
}
