//! The polkit authentication agent: polkitd asks it for a password whenever
//! something like pkexec or `systemctl restart` needs an admin

use std::{
  collections::{HashMap, VecDeque},
  ffi::CStr,
  io::{BufRead, BufReader, Write},
  os::unix::net::UnixStream,
  path::PathBuf,
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
  interface = "org.freedesktop.login1.Session",
  default_service = "org.freedesktop.login1",
  default_path = "/org/freedesktop/login1/session/auto"
)]
trait Session {
  #[zbus(property)]
  fn id(&self) -> zbus::Result<String>;
}

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
fn authenticate(helper: &Helper, user: &str, cookie: &str, password: &str) -> Result<bool> {
  if helper.socket.exists() {
    let socket = UnixStream::connect(&helper.socket)?;
    writeln!(&socket, "{user}\n{cookie}")?;
    return converse(BufReader::new(&socket), &socket, password);
  }
  let setuid = helper
    .setuid
    .iter()
    .find(|path| path.exists())
    .context("polkit has neither its helper socket nor polkit-agent-helper-1")?;
  let mut child = Command::new(setuid)
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

/// Where polkit's helper is
#[derive(Clone)]
struct Helper {
  socket: PathBuf,
  /// the first one there runs
  setuid: Vec<PathBuf>,
}

impl Default for Helper {
  fn default() -> Self {
    Self {
      socket: HELPER_SOCKET.into(),
      setuid: HELPERS.map(PathBuf::from).to_vec(),
    }
  }
}

/// The open requests, the shown one first
#[derive(Clone)]
pub struct Polkit {
  pub requests: Entity<VecDeque<PolkitRequest>>,
  helper: Helper,
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
    let (requests, helper) = (self.requests.clone(), self.helper.clone());
    cx.spawn(async move |cx| {
      let checked = cookie.clone();
      let right = cx
        .background_spawn(async move { authenticate(&helper, &user, &checked, &password) })
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
  // a process subject would only cover our own requests. `auto` is our
  // session, or the user's graphical one when run as a user service
  let session = SessionProxy::new(conn).await?.id().await?;
  let subject = (
    "unix-session",
    HashMap::from([("session-id", Value::from(session))]),
  );
  let locale = std::env::var("LANG").unwrap_or_else(|_| "C".into());
  AuthorityProxy::new(conn)
    .await?
    .register_authentication_agent(&subject, &locale, AGENT_PATH)
    .await?;
  Ok(rx)
}

pub(crate) fn init(cx: &mut App, conn: &Connection) {
  let polkit = Polkit {
    requests: cx.new(|_| VecDeque::new()),
    helper: Helper::default(),
  };
  serve(polkit.requests.clone(), conn.clone(), cx);
  cx.set_global(polkit);
}

/// Registers the agent on `conn`, its requests go into `requests`
fn serve(requests: Entity<VecDeque<PolkitRequest>>, conn: Connection, cx: &mut App) {
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

#[cfg(any(test, feature = "test-support"))]
impl Polkit {
  /// Without polkitd; `helper` speaks polkit-agent-helper-1's protocol: the
  /// user as its argument, the cookie, then PAM lines
  pub fn fake(helper: PathBuf, cx: &mut App) -> Self {
    Self {
      requests: cx.new(|_| VecDeque::new()),
      helper: Helper {
        socket: PathBuf::new(),
        setuid: vec![helper],
      },
    }
  }

  /// Asks like polkitd does; the receiver fails when the request is canceled
  pub fn ask(&self, message: &str, cookie: &str, cx: &mut App) -> oneshot::Receiver<()> {
    let (reply, done) = oneshot::channel();
    let request = PolkitRequest {
      message: message.into(),
      user: "alice".into(),
      cookie: cookie.into(),
      reply,
    };
    self.requests.update(cx, |requests, cx| {
      requests.push_back(request);
      cx.notify();
    });
    done
  }
}

#[cfg(test)]
mod tests {
  use std::{
    io::Cursor,
    os::unix::{fs::PermissionsExt, net::UnixListener},
    path::Path,
    sync::{Arc, Mutex},
    thread,
  };

  use corona_utils::test_bus::{TestBus, wait_until};
  use futures_lite::future::block_on;
  use gpui_kit::{self as gpui, TestAppContext};

  use super::*;

  fn uid() -> u32 {
    unsafe { libc::getuid() }
  }

  fn me() -> String {
    user_name(uid()).unwrap()
  }

  /// A setuid-less polkit-agent-helper-1 that only takes `user`, cookie `c1`
  /// and password `right`
  fn helper(dir: &Path, user: &str) -> PathBuf {
    let path = dir.join("helper");
    let script = format!(
      "#!/bin/sh\nread cookie\necho 'PAM_PROMPT_ECHO_OFF Password: '\nread password\n\
       [ \"$1 $cookie $password\" = '{user} c1 right' ] && echo SUCCESS || echo FAILURE\n"
    );
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
  }

  #[test]
  fn runs_the_setuid_helper() {
    let dir = tempfile::tempdir().unwrap();
    let fake = Helper {
      socket: dir.path().join("no-socket"),
      setuid: vec![dir.path().join("missing"), helper(dir.path(), "alice")],
    };
    assert!(authenticate(&fake, "alice", "c1", "right").unwrap());
    assert!(!authenticate(&fake, "alice", "c1", "wrong").unwrap());
    assert!(!authenticate(&fake, "bob", "c1", "right").unwrap());
    assert!(!authenticate(&fake, "alice", "c2", "right").unwrap());
    let none = Helper {
      socket: dir.path().join("no-socket"),
      setuid: vec![dir.path().join("missing")],
    };
    assert!(authenticate(&none, "alice", "c1", "right").is_err());
  }

  #[test]
  fn prefers_the_socket() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("socket");
    let listener = UnixListener::bind(&socket).unwrap();
    let polkitd = thread::spawn(move || {
      let (stream, _) = listener.accept().unwrap();
      let mut lines = BufReader::new(&stream).lines().map(Result::unwrap);
      let (user, cookie) = (lines.next().unwrap(), lines.next().unwrap());
      writeln!(&stream, "PAM_PROMPT_ECHO_OFF Password: ").unwrap();
      let password = lines.next().unwrap();
      writeln!(&stream, "SUCCESS").unwrap();
      [user, cookie, password]
    });
    // the setuid helper would refuse bob
    let fake = Helper {
      socket,
      setuid: vec![helper(dir.path(), "alice")],
    };
    assert!(authenticate(&fake, "bob", "c2", "pw").unwrap());
    assert_eq!(polkitd.join().unwrap(), ["bob", "c2", "pw"]);
  }

  fn identity(uid: u32) -> Identity {
    let details = HashMap::from([("uid".to_owned(), OwnedValue::from(uid))]);
    ("unix-user".to_owned(), details)
  }

  fn begin(agent: &Agent, identities: Vec<Identity>) -> Result<(), AgentError> {
    block_on(agent.begin_authentication(
      "org.example.do".into(),
      "Do it".into(),
      String::new(),
      HashMap::new(),
      "c1".into(),
      identities,
    ))
  }

  #[test]
  fn agent_answers() {
    let (events, mut received) = mpsc::unbounded();
    let agent = Agent { events };
    let dialog = thread::spawn(move || {
      let Some(Event::Begin(request)) = block_on(received.next()) else {
        panic!("no request");
      };
      assert_eq!(
        (request.message.as_str(), request.cookie.as_str()),
        ("Do it", "c1")
      );
      assert_eq!(request.user, me());
      request.reply.send(()).unwrap();
      // canceling drops the next one
      assert!(matches!(block_on(received.next()), Some(Event::Begin(_))));
      received
    });
    begin(&agent, vec![identity(uid())]).unwrap();
    let canceled = begin(&agent, vec![identity(uid())]);
    assert!(matches!(canceled, Err(AgentError::Cancelled(ref m)) if m == "canceled"));
    let mut received = dialog.join().unwrap();

    // nobody to ask for, polkitd never sends that
    assert!(matches!(begin(&agent, vec![]), Err(AgentError::Failed(_))));
    block_on(agent.cancel_authentication("c1".into()));
    assert!(matches!(received.try_recv(), Ok(Event::Cancel(c)) if c == "c1"));
    drop(received);
    let gone = begin(&agent, vec![identity(uid())]);
    assert!(matches!(gone, Err(AgentError::Cancelled(ref m)) if m == "shell is gone"));
  }

  struct FakePolkitd {
    registered: Arc<Mutex<Vec<String>>>,
  }

  #[zbus::interface(name = "org.freedesktop.PolicyKit1.Authority")]
  impl FakePolkitd {
    fn register_authentication_agent(
      &self,
      subject: (String, HashMap<String, OwnedValue>),
      locale: String,
      object_path: String,
    ) {
      let session = String::try_from(subject.1["session-id"].try_clone().unwrap()).unwrap();
      let registered = format!("{} {session} {locale} {object_path}", subject.0);
      self.registered.lock().unwrap().push(registered);
    }
  }

  struct FakeSession;

  #[zbus::interface(name = "org.freedesktop.login1.Session")]
  impl FakeSession {
    #[zbus(property)]
    fn id(&self) -> String {
      "4".into()
    }
  }

  #[proxy(
    interface = "org.freedesktop.PolicyKit1.AuthenticationAgent",
    default_path = "/org/freedesktop/PolicyKit1/AuthenticationAgent"
  )]
  trait AuthenticationAgent {
    fn begin_authentication(
      &self,
      action_id: &str,
      message: &str,
      icon_name: &str,
      details: HashMap<&str, &str>,
      cookie: &str,
      identities: &[(&str, HashMap<&str, Value<'_>>)],
    ) -> zbus::Result<()>;
    fn cancel_authentication(&self, cookie: &str) -> zbus::Result<()>;
  }

  async fn polkitd(bus: &TestBus, registered: &Arc<Mutex<Vec<String>>>) -> Connection {
    let services = bus.conn().await;
    let server = services.object_server();
    let polkitd = FakePolkitd {
      registered: registered.clone(),
    };
    server
      .at("/org/freedesktop/PolicyKit1/Authority", polkitd)
      .await
      .unwrap();
    server
      .at("/org/freedesktop/login1/session/auto", FakeSession)
      .await
      .unwrap();
    services
      .request_name("org.freedesktop.PolicyKit1")
      .await
      .unwrap();
    services
      .request_name("org.freedesktop.login1")
      .await
      .unwrap();
    services
  }

  /// Asks the agent over the bus like polkitd, answered on another thread
  fn ask(
    caller: &Connection,
    agent: &Connection,
    cookie: &'static str,
  ) -> thread::JoinHandle<zbus::Result<()>> {
    let caller = caller.clone();
    let destination = agent.unique_name().unwrap().to_owned();
    thread::spawn(move || {
      block_on(async {
        let agent = AuthenticationAgentProxy::builder(&caller)
          .destination(destination)?
          .build()
          .await?;
        let uid = HashMap::from([("uid", Value::from(uid()))]);
        agent
          .begin_authentication(
            "org.example.do",
            "Do it",
            "",
            HashMap::new(),
            cookie,
            &[("unix-user", uid)],
          )
          .await
      })
    })
  }

  fn cookies(polkit: &Polkit, cx: &mut TestAppContext) -> Vec<String> {
    polkit.requests.read_with(cx, |requests, _| {
      requests.iter().map(|r| r.cookie.clone()).collect()
    })
  }

  fn canceled(asked: thread::JoinHandle<zbus::Result<()>>) -> bool {
    let error = asked.join().unwrap().unwrap_err();
    matches!(error, zbus::Error::MethodError(ref name, ..) if name.as_str() == "org.freedesktop.PolicyKit1.Error.Cancelled")
  }

  #[gpui::test]
  fn over_the_bus(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let bus = TestBus::new();
    let dir = tempfile::tempdir().unwrap();
    let registered = Arc::default();
    let (_services, agent, caller) = block_on(async {
      let services = polkitd(&bus, &registered).await;
      (services, bus.conn().await, bus.conn().await)
    });
    let polkit = cx.update(|cx| {
      let polkit = Polkit::fake(helper(dir.path(), &me()), cx);
      serve(polkit.requests.clone(), agent.clone(), cx);
      polkit
    });
    wait_until(cx, |_| !registered.lock().unwrap().is_empty());
    let locale = std::env::var("LANG").unwrap_or_else(|_| "C".into());
    assert_eq!(
      *registered.lock().unwrap(),
      [format!("unix-session 4 {locale} {AGENT_PATH}")]
    );

    // queued in order, the first is shown
    let first = ask(&caller, &agent, "c1");
    wait_until(cx, |cx| cookies(&polkit, cx).len() == 1);
    let second = ask(&caller, &agent, "c2");
    let third = ask(&caller, &agent, "c3");
    wait_until(cx, |cx| cookies(&polkit, cx).len() == 3);
    assert_eq!(cookies(&polkit, cx)[0], "c1");
    assert_eq!(cx.read(|cx| polkit.request(cx).unwrap().user.clone()), me());

    // polkitd giving up on a queued one
    block_on(async {
      AuthenticationAgentProxy::builder(&caller)
        .destination(agent.unique_name().unwrap().to_owned())
        .unwrap()
        .build()
        .await
        .unwrap()
        .cancel_authentication("c2")
        .await
        .unwrap()
    });
    wait_until(cx, |cx| cookies(&polkit, cx).len() == 2);
    assert!(canceled(second));

    let check = |password: &str, cx: &mut TestAppContext| {
      let task = cx.update(|cx| polkit.authenticate(password.into(), cx));
      cx.run_until_parked();
      block_on(task).unwrap()
    };
    // a wrong password leaves it open for another try
    assert!(!check("wrong", cx));
    assert_eq!(cookies(&polkit, cx), ["c1", "c3"]);
    assert!(check("right", cx));
    assert_eq!(cookies(&polkit, cx), ["c3"]);
    first.join().unwrap().unwrap();

    // the dialog's cancel
    cx.update(|cx| polkit.cancel(cx));
    assert!(cookies(&polkit, cx).is_empty());
    assert!(canceled(third));
    assert!(!check("right", cx));
  }

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
