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
  time::Duration,
};

use anyhow::{Context as _, Result, anyhow, ensure};
use corona_js::{
  Cx, Data, Json, Listener, ModuleBuilder, Opt, Plugin, PluginRef, ScriptModule, Throws,
  Unsubscribe, host_module,
};
use futures::stream::{FuturesUnordered, StreamExt as _};
use futures_lite::{AsyncReadExt as _, AsyncWriteExt as _, future};
use gpui_kit::{App, PathPromptOptions, Subscription};
use serde::{Deserialize, Serialize};
use smol::net::{TcpListener, TcpStream};
use specta::{Type, specta};

#[derive(Default, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
struct PickFiles {
  #[specta(optional)]
  multiple: Option<bool>,
  /// Pick directories instead of files.
  #[specta(optional)]
  directory: Option<bool>,
  /// The prompt on the dialog.
  #[specta(optional)]
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

#[derive(Default, Deserialize, Serialize, Type)]
#[serde(rename_all = "camelCase")]
struct ListenRedirect {
  /// How long `onRedirect` waits, 5 minutes by default, 10 at most.
  #[specta(optional)]
  timeout_ms: Option<u32>,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
struct Listening {
  id: u32,
  port: u16,
  /// `http://127.0.0.1:<port>/`, any path on it is taken.
  redirect_uri: String,
}

/// The request the browser was redirected with.
#[derive(Debug, PartialEq, Serialize, Type)]
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

/// What `onRedirect` hands the plugin: the redirect, or why there is none.
#[derive(Serialize, Type)]
#[serde(untagged)]
enum RedirectEvent {
  Redirect(Redirect),
  Failed { message: String },
}

/// A runtime's redirect listeners, waiting to be taken by `onRedirect`
#[derive(Default)]
struct Redirects {
  listeners: RefCell<HashMap<u32, (TcpListener, Duration)>>,
  /// Listeners made, taken or not, and still open
  open: Cell<usize>,
  next_id: Cell<u32>,
}

/// Asks the user for files, null when they cancel. What they pick is readable by this
/// plugin until it restarts, a directory with everything in it.
#[specta]
async fn pick_files(
  cx: Cx,
  plugin: Plugin,
  options: Opt<Json<PickFiles>>,
) -> Throws<Json<Option<Vec<String>>>> {
  let options = options.0.map(|o| o.0).unwrap_or_default();
  let directory = options.directory.unwrap_or(false);
  let paths = cx.prompt_for_paths(PathPromptOptions {
    files: !directory,
    directories: directory,
    multiple: options.multiple.unwrap_or(false),
    prompt: options.accept_label.map(Into::into),
  });
  let picked = async { anyhow::Ok(paths.await??) }.await;
  let shown = picked.map(|paths| {
    paths.map(|paths| {
      (paths.into_iter())
        .map(|path| {
          let shown = path.to_string_lossy().into_owned();
          plugin.capabilities.grant_read(path);
          shown
        })
        .collect()
    })
  });
  shown.map(Json).into()
}

/// Opens a file the plugin may read in the user's default app.
#[specta]
fn open_path(cx: Cx, plugin: Plugin, path: String) -> Throws<()> {
  let open = || -> Result<()> {
    let path = openable(Path::new(&path))?;
    let readable = plugin.capabilities.may_read(&path);
    ensure!(readable, "`{}` is not readable", path.display());
    cx.open_with_system(&path);
    Ok(())
  };
  open().into()
}

/// Opens a URI, e.g. `https:` or `mailto:`, in the user's default app.
#[specta]
fn open_uri(cx: Cx, uri: String) -> Throws<()> {
  uri_ok(&uri).map(|()| cx.open_url(&uri)).into()
}

/// Starts an app, `argv[0]` one the plugin may execute, as the launcher does: in its own
/// scope with the user's whole session environment, not the cleared one of `process`.
#[specta]
fn launch(plugin: Plugin, argv: Vec<String>) -> Throws<()> {
  let launch = || -> Result<()> {
    let program = argv.first().context("nothing to launch")?;
    ensure!(
      plugin.capabilities.may_run(program),
      "`{program}` may not be executed"
    );
    corona_desktop::apps::spawn(&plugin.id, &argv, None)
  };
  launch().into()
}

/// Listens on 127.0.0.1 for one OAuth redirect, for a login opened with `openUri`, on a
/// free port. Take it with `onRedirect(id, cb)`.
#[specta]
fn listen_redirect(
  redirects: Data<Redirects>,
  options: Opt<Json<ListenRedirect>>,
) -> Throws<Json<Listening>> {
  let listen = || -> Result<Listening> {
    let options = options.0.map(|o| o.0).unwrap_or_default();
    ensure!(
      redirects.open.get() < MAX_LISTENERS,
      "at most {MAX_LISTENERS} redirect listeners at once"
    );
    let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let port = listener.local_addr()?.port();
    let id = redirects.next_id.get() + 1;
    redirects.next_id.set(id);
    let timeout = options
      .timeout_ms
      .map_or(TIMEOUT, |ms| Duration::from_millis(ms.into()))
      .min(MAX_TIMEOUT);
    let listener = (TcpListener::try_from(listener)?, timeout);
    redirects.listeners.borrow_mut().insert(id, listener);
    redirects.open.set(redirects.open.get() + 1);
    Ok(Listening {
      id,
      port,
      redirect_uri: format!("http://127.0.0.1:{port}/"),
    })
  };
  listen().map(Json).into()
}

/// Calls `cb` once with the redirect listener `id` takes, or why there is none; the
/// browser gets a page saying the tab can be closed. The port closes after it or the
/// timeout.
#[specta]
fn on_redirect(
  cx: Cx,
  redirects: Data<Redirects>,
  id: u32,
  cb: Listener<Json<RedirectEvent>>,
) -> Throws<Unsubscribe> {
  let taken = redirects.listeners.borrow_mut().remove(&id);
  let Some((listener, timeout)) = taken else {
    return Err::<Unsubscribe, _>("no such listener, or it was used").into();
  };
  let listener_cb = cb.clone();
  // Whichever of the finished task and the closed subscription comes first gives the
  // slot back; the other must not.
  let released = Rc::new(Cell::new(false));
  let release = {
    let redirects = redirects.clone();
    move || {
      if !released.replace(true) {
        redirects.open.set(redirects.open.get() - 1);
      }
    }
  };
  let done = release.clone();
  let task = cx.spawn(async move |cx| {
    let timed_out = async {
      smol::Timer::after(timeout).await;
      Err(anyhow!("no redirect within {timeout:?}"))
    };
    let event = match future::or(redirect(listener), timed_out).await {
      Ok(redirect) => RedirectEvent::Redirect(redirect),
      Err(e) => RedirectEvent::Failed {
        message: e.to_string(),
      },
    };
    cx.update(|cx| listener_cb.call(cx, Json(event)));
    done();
  });
  let close = Subscription::new(move || {
    drop(task);
    release();
  });
  Ok::<_, anyhow::Error>(cb.keep(close)).into()
}

/// `corona/desktop`
pub struct Desktop;

impl ScriptModule for Desktop {
  const NAME: &str = "desktop";
  const DOC: &str =
    "The file picker, launching apps and opening files and URIs in the user's apps.";

  fn module(_: &PluginRef, m: &mut ModuleBuilder, _: &mut App) {
    m.provide(Redirects::default());
    host_module!(m, {
      sync: [open_path, open_uri, launch, listen_redirect, on_redirect],
      async: [pick_files],
    });
  }
}

#[cfg(test)]
mod tests {
  use corona_utils::test_bus::wait_until;
  use gpui_kit::{self as gpui, TestAppContext};

  use super::*;
  use crate::harness;

  #[gpui::test]
  fn refused_uris_never_reach_the_platform(cx: &mut TestAppContext) {
    let body = r#"report([
      attempt(() => m.openUri("file:///etc/passwd")),
      attempt(() => m.openPath("a.txt")),
      attempt(() => m.openPath("/etc/hostname")),
    ]);"#;
    let (view, cx) = harness::module::<Desktop>(cx, body);
    let reports = view.last(cx);
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
  fn launches_only_granted_programs(cx: &mut TestAppContext) {
    let body = r#"report([attempt(() => m.launch([])), attempt(() => m.launch(["sh"]))]);"#;
    let (view, cx) = harness::module::<Desktop>(cx, body);
    let reports = view.last(cx);
    let message = |i: usize| reports[i]["message"].as_str().unwrap().to_string();
    assert!(message(0).contains("nothing to launch"), "{}", message(0));
    assert!(message(1).contains("may not be executed"), "{}", message(1));
  }

  #[gpui::test]
  fn opens_uris(cx: &mut TestAppContext) {
    let body = r#"report(attempt(() => m.openUri("mailto:a@b.c")));"#;
    let (view, cx) = harness::module::<Desktop>(cx, body);
    assert_eq!(view.last(cx), serde_json::Value::Null);
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
      report({ ...l, capped: attempt(() => m.listenRedirect()), freePort: t.port !== 1 });
      m.onRedirect(l.id, report);
      report(attempt(() => m.onRedirect(l.id, report)));
      m.onRedirect(t.id, report);
    }"#;
    let (view, cx) = harness::module::<Desktop>(cx, body);
    wait_until(cx, |cx| view.reports(cx).len() == 3);
    let listening = view.reports(cx)[0].clone();
    let capped = listening["capped"]["message"].as_str().unwrap();
    assert!(capped.contains("at most"), "{capped}");
    assert_eq!(listening["freePort"], true);
    let port = listening["port"].as_u64().unwrap();
    assert_eq!(
      listening["redirectUri"],
      format!("http://127.0.0.1:{port}/")
    );
    let errors: Vec<String> = view.reports(cx)[1..]
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
    wait_until(cx, |cx| view.reports(cx).len() == 4);
    assert_eq!(
      view.last(cx),
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
