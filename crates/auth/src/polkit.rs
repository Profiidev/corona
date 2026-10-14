//! The polkit authentication agent: polkitd asks it for a password whenever
//! something like pkexec or `systemctl restart` needs an admin

use std::{
  collections::{HashMap, VecDeque},
  ffi::CStr,
  io::{BufRead, BufReader, Write},
  os::unix::net::UnixStream,
  path::Path,
  process::{Command, Stdio},
};

use anyhow::{Context as _, Result, bail};
use futures::channel::{mpsc, oneshot};
use futures_lite::StreamExt;
use gpui_kit::{App, AppContext, Entity, Global, Task};
use zbus::{
  Connection, proxy,
  zvariant::{OwnedValue, Value},
};

const AGENT_PATH: &str = "/org/freedesktop/PolicyKit1/AuthenticationAgent";
/// polkit 126 and newer start a helper per connection here
const HELPER_SOCKET: &str = "/run/polkit/agent-helper.socket";
/// The setuid helper of older polkit, NixOS keeps it in its wrappers
const HELPERS: [&str; 3] = [
  "/run/wrappers/bin/polkit-agent-helper-1",
  "/usr/lib/polkit-1/polkit-agent-helper-1",
  "/usr/libexec/polkit-agent-helper-1",
];

#[proxy(
  interface = "org.freedesktop.PolicyKit1.Authority",
  default_service = "org.freedesktop.PolicyKit1",
  default_path = "/org/freedesktop/PolicyKit1/Authority"
)]
trait Authority {
  fn register_authentication_agent(
    &self,
    subject: &(&str, HashMap<&str, Value<'_>>),
    locale: &str,
    object_path: &str,
  ) -> zbus::Result<()>;
}

/// polkitd wants someone to prove they are `user`, answer it with
/// `Polkit::authenticate` or `Polkit::cancel`
pub struct PolkitRequest {
  /// What is about to happen, already translated by polkitd
  pub message: String,
  pub user: String,
  /// Tells requests apart
  pub cookie: String,
  /// Dropping it fails the request as canceled
  reply: oneshot::Sender<()>,
}

enum Event {
  Begin(PolkitRequest),
  Cancel(String),
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.freedesktop.PolicyKit1.Error")]
enum AgentError {
  #[zbus(error)]
  ZBus(zbus::Error),
  Failed(String),
  Cancelled(String),
}

struct Agent {
  events: mpsc::UnboundedSender<Event>,
}

type Identity = (String, HashMap<String, OwnedValue>);

#[zbus::interface(name = "org.freedesktop.PolicyKit1.AuthenticationAgent")]
impl Agent {
  async fn begin_authentication(
    &self,
    _action_id: String,
    message: String,
    _icon_name: String,
    _details: HashMap<String, String>,
    cookie: String,
    identities: Vec<Identity>,
  ) -> Result<(), AgentError> {
    let me = unsafe { libc::getuid() };
    let user = pick_user(&identities, me)
      .and_then(user_name)
      .ok_or_else(|| AgentError::Failed("no user to authenticate as".into()))?;
    let (reply, done) = oneshot::channel();
    let request = PolkitRequest {
      message,
      user,
      cookie,
      reply,
    };
    self
      .events
      .unbounded_send(Event::Begin(request))
      .map_err(|_| AgentError::Cancelled("shell is gone".into()))?;
    done
      .await
      .map_err(|_| AgentError::Cancelled("canceled".into()))
  }

  async fn cancel_authentication(&self, cookie: String) {
    let _ = self.events.unbounded_send(Event::Cancel(cookie));
  }
}

/// Ourselves when we may answer (an admin), otherwise the first admin.
/// ponytail: unix-group identities are skipped, polkitd sends users for
/// wheel rules anyway
fn pick_user(identities: &[Identity], me: u32) -> Option<u32> {
  let uids: Vec<u32> = identities
    .iter()
    .filter(|(kind, _)| kind == "unix-user")
    .filter_map(|(_, details)| details.get("uid")?.downcast_ref::<u32>().ok())
    .collect();
  match uids.contains(&me) {
    true => Some(me),
    false => uids.first().copied(),
  }
}

fn user_name(uid: u32) -> Option<String> {
  // SAFETY: an all zero passwd is valid, getpwuid_r fills it
  let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
  let mut buf = vec![0 as libc::c_char; 4096];
  let mut found = std::ptr::null_mut();
  // SAFETY: every pointer outlives the call; reentrant, unlike getpwuid
  let code = unsafe { libc::getpwuid_r(uid, &mut entry, buf.as_mut_ptr(), buf.len(), &mut found) };
  if code != 0 || found.is_null() {
    return None;
  }
  // SAFETY: pw_name points into buf, which is still alive
  let name = unsafe { CStr::from_ptr(entry.pw_name) };
  Some(name.to_string_lossy().into_owned())
}

/// Runs polkit's helper for `user`, which tells polkitd itself when the
/// password was right. `Ok(false)` for a wrong one.
fn authenticate(user: &str, cookie: &str, password: &str) -> Result<bool> {
  if Path::new(HELPER_SOCKET).exists() {
    let socket = UnixStream::connect(HELPER_SOCKET)?;
    writeln!(&socket, "{user}\n{cookie}")?;
    return converse(BufReader::new(&socket), &socket, password);
  }
  let helper = HELPERS
    .into_iter()
    .find(|path| Path::new(path).exists())
    .context("polkit has neither its helper socket nor polkit-agent-helper-1")?;
  let mut child = Command::new(helper)
    .arg(user)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .spawn()?;
  let mut stdin = child.stdin.take().context("no helper stdin")?;
  writeln!(stdin, "{cookie}")?;
  let stdout = child.stdout.take().context("no helper stdout")?;
  // stdin closes with the conversation, so the helper exits before the wait
  let result = converse(BufReader::new(stdout), stdin, password);
  let _ = child.wait();
  result
}

/// The helper's PAM conversation, one message per line
fn converse(output: impl BufRead, mut input: impl Write, password: &str) -> Result<bool> {
  for line in output.lines() {
    let line = line?;
    match line.split(' ').next().unwrap_or_default() {
      "PAM_PROMPT_ECHO_OFF" | "PAM_PROMPT_ECHO_ON" => writeln!(input, "{password}")?,
      "SUCCESS" => return Ok(true),
      "FAILURE" => return Ok(false),
      // PAM_ERROR_MSG and PAM_TEXT_INFO
      _ => {}
    }
  }
  bail!("polkit's helper quit without an answer")
}

/// The open requests, the shown one first
#[derive(Clone)]
pub struct Polkit {
  pub requests: Entity<VecDeque<PolkitRequest>>,
}

impl Global for Polkit {}

impl Polkit {
  pub fn request<'c>(&self, cx: &'c App) -> Option<&'c PolkitRequest> {
    self.requests.read(cx).front()
  }

  /// Answers the shown request. `Ok(false)` for a wrong password, which
  /// leaves the request open for another try
  pub fn authenticate(&self, password: String, cx: &App) -> Task<Result<bool>> {
    let Some(request) = self.request(cx) else {
      return Task::ready(Ok(false));
    };
    let (user, cookie) = (request.user.clone(), request.cookie.clone());
    let requests = self.requests.clone();
    cx.spawn(async move |cx| {
      let checked = cookie.clone();
      let right = cx
        .background_spawn(async move { authenticate(&user, &checked, &password) })
        .await?;
      if right {
        requests.update(cx, |requests, cx| {
          if let Some(i) = requests.iter().position(|r| r.cookie == cookie)
            && let Some(request) = requests.remove(i)
          {
            let _ = request.reply.send(());
            cx.notify();
          }
        });
      }
      Ok(right)
    })
  }

  /// Gives up on the shown request
  pub fn cancel(&self, cx: &mut App) {
    self.requests.update(cx, |requests, cx| {
      requests.pop_front();
      cx.notify();
    });
  }
}

async fn register(conn: &Connection) -> Result<mpsc::UnboundedReceiver<Event>> {
  let (events, rx) = mpsc::unbounded();
  conn
    .object_server()
    .at(AGENT_PATH, Agent { events })
    .await?;
  // a shell run as a user service has no session of its own, polkitd looks
  // the session up from our pid then
  let subject = match std::env::var("XDG_SESSION_ID") {
    Ok(id) => (
      "unix-session",
      HashMap::from([("session-id", Value::from(id))]),
    ),
    Err(_) => (
      "unix-process",
      HashMap::from([
        ("pid", Value::from(std::process::id())),
        ("start-time", Value::from(0u64)),
      ]),
    ),
  };
  let locale = std::env::var("LANG").unwrap_or_else(|_| "C".into());
  AuthorityProxy::new(conn)
    .await?
    .register_authentication_agent(&subject, &locale, AGENT_PATH)
    .await?;
  Ok(rx)
}

pub(crate) fn init(cx: &mut App, conn: &Connection) {
  let requests = cx.new(|_| VecDeque::new());
  cx.set_global(Polkit {
    requests: requests.clone(),
  });
  let conn = conn.clone();
  cx.spawn(async move |cx| {
    // another agent, like polkit-gnome, keeps asking; everything else works
    let mut events = match register(&conn).await {
      Ok(events) => events,
      Err(e) => return tracing::warn!("Not answering polkit: {e:#}"),
    };
    while let Some(event) = events.next().await {
      requests.update(cx, |requests, cx| {
        match event {
          Event::Begin(request) => requests.push_back(request),
          Event::Cancel(cookie) => requests.retain(|r| r.cookie != cookie),
        }
        cx.notify();
      });
    }
  })
  .detach();
}

#[cfg(test)]
mod tests {
  use std::io::Cursor;

  use super::*;

  fn talk(helper: &str) -> (Result<bool>, String) {
    let mut sent = Vec::new();
    let result = converse(Cursor::new(helper), &mut sent, "hunter2");
    (result, String::from_utf8(sent).unwrap())
  }

  #[test]
  fn conversations() {
    let (result, sent) = talk("PAM_TEXT_INFO hi\nPAM_PROMPT_ECHO_OFF Password: \nSUCCESS\n");
    assert!(result.unwrap());
    assert_eq!(sent, "hunter2\n");
    let (result, _) = talk("PAM_PROMPT_ECHO_OFF Password: \nPAM_ERROR_MSG nope\nFAILURE\n");
    assert!(!result.unwrap());
    assert!(talk("PAM_PROMPT_ECHO_OFF Password: \n").0.is_err());
  }

  #[test]
  fn picks_users() {
    let user = |uid: u32| {
      let details = HashMap::from([("uid".to_owned(), OwnedValue::from(uid))]);
      ("unix-user".to_owned(), details)
    };
    let group = ("unix-group".to_owned(), HashMap::new());
    let admins = vec![group, user(0), user(1000)];
    assert_eq!(pick_user(&admins, 1000), Some(1000));
    assert_eq!(pick_user(&admins, 1001), Some(0));
    assert_eq!(pick_user(&[], 1000), None);
    assert_eq!(user_name(0).as_deref(), Some("root"));
  }
}
