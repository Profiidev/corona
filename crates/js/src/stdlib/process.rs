//! `process`: `run`/`spawn` of granted commands with a cleared environment, bounded IO, and
//! children killed with their process group when their handle is dropped.

use std::{
  os::unix::process::ExitStatusExt,
  path::Path,
  process::{ExitStatus, Stdio},
  sync::{Arc, PoisonError},
  time::Duration,
};

use rquickjs::{
  Ctx, Exception, Function, IntoJs, JsLifetime, Object, Promise, Result, Value,
  class::Trace,
  function::Opt,
  module::{Declarations, Exports, ModuleDef},
};
use tokio::{
  io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader},
  process::{ChildStderr, ChildStdin, ChildStdout, Command},
  sync::{Mutex, watch},
};

use super::io::{Fail, Null, Res, detach, enter, io, promise};
use crate::{
  app,
  runtime::{
    caps::{Access, CapabilityError},
    plugin::plugin,
    runtime,
  },
};

const MAX_OUTPUT: u64 = 8 * 1024 * 1024;
const MAX_LINE: usize = 1024 * 1024;
const MAX_WRITE: usize = 1024 * 1024;
/// Between SIGTERM and SIGKILL when a child's handle is dropped
const GRACE: Duration = Duration::from_secs(2);

/// Validated `options`, for `run` (no stream options) or `spawn`
struct Options {
  env: Vec<(String, String)>,
  cwd: Option<String>,
  stdout: bool,
  stderr: bool,
}

fn options(ctx: &Ctx<'_>, options: Option<Object<'_>>, spawn: bool) -> Res<Options> {
  let mut out = Options {
    env: Vec::new(),
    cwd: None,
    stdout: true,
    stderr: true,
  };
  let Some(options) = options else {
    return Ok(out);
  };
  let caps = plugin(ctx).map_err(|_| Fail::Type("no plugin runs here".into()))?;
  let caps = &caps.capabilities;
  let js = |e: rquickjs::Error| Fail::Type(e.to_string());
  for key in options.keys::<String>() {
    let key = key.map_err(js)?;
    let value: Value = options.get(&key).map_err(js)?;
    match key.as_str() {
      "env" => {
        let env = value
          .as_object()
          .ok_or_else(|| Fail::Type("process options.env expects an object".into()))?;
        for name in env.keys::<String>() {
          let name = name.map_err(js)?;
          let value: Value = env.get(&name).map_err(js)?;
          let Some(value) = value.as_string() else {
            return Err(Fail::Type(
              "process options.env expects string values".into(),
            ));
          };
          if !caps.may_pass_env(&name) {
            return Err(CapabilityError::EnvDenied(name).into());
          }
          out.env.push((name, value.to_string().map_err(js)?));
        }
      }
      "cwd" => {
        let cwd: String = value
          .as_string()
          .ok_or_else(|| Fail::Type("process options.cwd expects a string".into()))?
          .to_string()
          .map_err(js)?;
        if !Path::new(&cwd).is_absolute() {
          return Err(Fail::Type(format!(
            "process options.cwd `{cwd}` must be an absolute path"
          )));
        }
        caps.open(Path::new(&cwd), Access::Read)?;
        out.cwd = Some(cwd);
      }
      "stdout" | "stderr" if !spawn => {
        return Err(Fail::Type(
          "process.run(command, args, options) captures both streams; use spawn to choose".into(),
        ));
      }
      "stdout" | "stderr" => {
        let mode: String = value.get().map_err(js)?;
        let pipe = match mode.as_str() {
          "pipe" => true,
          "ignore" => false,
          other => {
            return Err(Fail::Type(format!(
              "process options.{key} `{other}` must be \"pipe\" or \"ignore\""
            )));
          }
        };
        match key.as_str() {
          "stdout" => out.stdout = pipe,
          _ => out.stderr = pipe,
        }
      }
      other => return Err(Fail::Type(format!("unknown process option `{other}`"))),
    }
  }
  Ok(out)
}

/// A command for `command` with the grant of the calling plugin
fn command(ctx: &Ctx<'_>, command: &str, args: Vec<String>, options: &Options) -> Res<Command> {
  let caps = plugin(ctx).map_err(|_| Fail::Type("no plugin runs here".into()))?;
  let caps = &caps.capabilities;
  if !caps.may_run(command) {
    return Err(CapabilityError::ExecuteDenied(command.into()).into());
  }
  let mut cmd = Command::new(command);
  cmd.args(args).env_clear().process_group(0);
  for name in caps.process_env_names() {
    if let Some(value) = std::env::var_os(name) {
      cmd.env(name, value);
    }
  }
  cmd.envs(options.env.iter().map(|(k, v)| (k, v)));
  if let Some(cwd) = &options.cwd {
    cmd.current_dir(cwd);
  }
  Ok(cmd)
}

fn args(args: Opt<Vec<String>>) -> Vec<String> {
  args.0.unwrap_or_default()
}

struct Output {
  code: i32,
  stdout: String,
  stderr: String,
}

impl<'js> IntoJs<'js> for Output {
  fn into_js(self, ctx: &Ctx<'js>) -> Result<Value<'js>> {
    let o = Object::new(ctx.clone())?;
    o.set("code", self.code)?;
    o.set("stdout", self.stdout)?;
    o.set("stderr", self.stderr)?;
    Ok(o.into_value())
  }
}

async fn all(stream: impl AsyncRead + Unpin, name: &str) -> Res<String> {
  let mut out = Vec::new();
  let read = stream.take(MAX_OUTPUT + 1).read_to_end(&mut out).await;
  read.map_err(|e| Fail::Error(format!("reading the child's {name} failed: {e}")))?;
  if out.len() as u64 > MAX_OUTPUT {
    return Err(Fail::Error(format!(
      "process {name} exceeded the {MAX_OUTPUT} byte limit"
    )));
  }
  Ok(String::from_utf8_lossy(&out).into_owned())
}

/// The process groups of the children a runtime started, ended with it: disabling a plugin
/// ends what its commands left running. ponytail: a group that emptied and whose id a new
/// group leader took since is hit too; pidfds or a cgroup if that ever matters
#[derive(Default)]
pub(crate) struct ChildGroups(std::sync::Mutex<Vec<i32>>);

fn alive(group: i32) -> bool {
  unsafe { libc::kill(-group, 0) == 0 }
}

impl ChildGroups {
  fn add(&self, pid: u32) {
    let mut groups = self.0.lock().unwrap_or_else(PoisonError::into_inner);
    groups.retain(|g| alive(*g));
    groups.push(pid as i32);
  }

  /// SIGTERM to every group still alive, SIGKILL after the grace period
  pub(crate) fn terminate(&self) {
    let mut groups = self.0.lock().unwrap_or_else(PoisonError::into_inner);
    let groups: Vec<i32> = groups.drain(..).filter(|g| alive(*g)).collect();
    if groups.is_empty() {
      return;
    }
    for group in &groups {
      unsafe { libc::kill(-group, libc::SIGTERM) };
    }
    detach(async move {
      tokio::time::sleep(GRACE).await;
      for group in groups.into_iter().filter(|g| alive(*g)) {
        unsafe { libc::kill(-group, libc::SIGKILL) };
      }
    });
  }
}

/// SIGKILLs the process group of a `run` child that is dropped before it finished
struct KillGroup(Option<u32>);

impl Drop for KillGroup {
  fn drop(&mut self) {
    if let Some(pid) = self.0 {
      unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
    }
  }
}

fn run<'js>(
  ctx: Ctx<'js>,
  name: String,
  args_: Opt<Vec<String>>,
  opts: Opt<Object<'js>>,
) -> Result<Promise<'js>> {
  let opts = options(&ctx, opts.0, false).map_err(|f| f.throw(&ctx))?;
  let mut cmd = command(&ctx, &name, args(args_), &opts).map_err(|f| f.throw(&ctx))?;
  let groups = runtime(&ctx)?.groups.clone();
  cmd
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);
  promise(&ctx, async move {
    io(async move {
      let mut child = cmd
        .spawn()
        .map_err(|e| Fail::Error(format!("starting `{name}` failed: {e}")))?;
      let mut group = KillGroup(child.id());
      if let Some(pid) = child.id() {
        groups.add(pid);
      }
      let (stdout, stderr) = (child.stdout.take(), child.stderr.take());
      let (stdout, stderr) = tokio::try_join!(
        all(stdout.expect("piped"), "stdout"),
        all(stderr.expect("piped"), "stderr")
      )?;
      let status = child
        .wait()
        .await
        .map_err(|e| Fail::Error(format!("waiting for `{name}` failed: {e}")))?;
      // what a finished run left in the background is meant to stay
      group.0 = None;
      Ok(Output {
        code: status.code().unwrap_or(-1),
        stdout,
        stderr,
      })
    })
    .await
  })
}

/// `{ code, signal }`
#[derive(Clone, Copy)]
struct Status(ExitStatus);

impl<'js> IntoJs<'js> for Status {
  fn into_js(self, ctx: &Ctx<'js>) -> Result<Value<'js>> {
    let o = Object::new(ctx.clone())?;
    o.set("code", Null(self.0.code()))?;
    o.set("signal", Null(self.0.signal()))?;
    Ok(o.into_value())
  }
}

type Lines<R> = Arc<Mutex<Option<BufReader<R>>>>;

/// The next line of `reader` without its newline; null at EOF or when ignored
async fn line<R: AsyncRead + Unpin>(reader: Lines<R>) -> Res<Option<String>> {
  let mut reader = reader.lock().await;
  let Some(reader) = reader.as_mut() else {
    return Ok(None);
  };
  let mut out = Vec::new();
  let mut over = false;
  loop {
    let buf = reader
      .fill_buf()
      .await
      .map_err(|e| Fail::Error(format!("reading the child failed: {e}")))?;
    if buf.is_empty() {
      if out.is_empty() && !over {
        return Ok(None);
      }
      break;
    }
    let (take, done) = match buf.iter().position(|b| *b == b'\n') {
      Some(i) => (i, i + 1),
      None => (buf.len(), buf.len()),
    };
    // past the limit the rest of the line is dropped, so the child is not blocked on it
    if !over {
      out.extend_from_slice(&buf[..take]);
      over = out.len() > MAX_LINE;
    }
    let newline = take < done;
    reader.consume(done);
    if newline {
      break;
    }
  }
  if over {
    return Err(Fail::Error(format!(
      "a child output line exceeded {MAX_LINE} bytes"
    )));
  }
  Ok(Some(String::from_utf8_lossy(&out).into_owned()))
}

/// A running child, leader of its own process group
#[derive(Trace, JsLifetime)]
#[rquickjs::class]
pub struct ChildProcess {
  #[qjs(skip_trace)]
  pid: u32,
  #[qjs(skip_trace)]
  stdout: Lines<ChildStdout>,
  #[qjs(skip_trace)]
  stderr: Lines<ChildStderr>,
  /// `None` once closed
  #[qjs(skip_trace)]
  stdin: Arc<Mutex<Option<ChildStdin>>>,
  #[qjs(skip_trace)]
  status: watch::Receiver<Option<ExitStatus>>,
}

fn signal(name: &str) -> Option<i32> {
  Some(match name {
    "SIGTERM" => libc::SIGTERM,
    "SIGKILL" => libc::SIGKILL,
    "SIGINT" => libc::SIGINT,
    "SIGHUP" => libc::SIGHUP,
    _ => return None,
  })
}

#[rquickjs::methods]
impl ChildProcess {
  #[qjs(get)]
  pub fn pid(&self) -> u32 {
    self.pid
  }

  #[qjs(rename = "readLine")]
  pub fn read_line<'js>(&self, ctx: Ctx<'js>) -> Result<Promise<'js>> {
    let stdout = self.stdout.clone();
    promise(&ctx, async move { io(line(stdout)).await.map(Null) })
  }

  #[qjs(rename = "readErrLine")]
  pub fn read_err_line<'js>(&self, ctx: Ctx<'js>) -> Result<Promise<'js>> {
    let stderr = self.stderr.clone();
    promise(&ctx, async move { io(line(stderr)).await.map(Null) })
  }

  pub fn write<'js>(&self, ctx: Ctx<'js>, data: Value<'js>) -> Result<Promise<'js>> {
    let data = super::io::bytes(&data, "child.write(data)")?;
    if data.len() > MAX_WRITE {
      return Err(Exception::throw_range(
        &ctx,
        &format!("child.write(data) exceeded the {MAX_WRITE} byte limit"),
      ));
    }
    let stdin = self.stdin.clone();
    promise(
      &ctx,
      io(async move {
        let mut stdin = stdin.lock().await;
        let stdin = stdin
          .as_mut()
          .ok_or_else(|| Fail::Error("the child's stdin is closed".into()))?;
        let write = async {
          stdin.write_all(&data).await?;
          stdin.flush().await
        };
        write
          .await
          .map_err(|e| Fail::Error(format!("writing to the child failed: {e}")))
      }),
    )
  }

  #[qjs(rename = "closeStdin")]
  pub fn close_stdin<'js>(&self, ctx: Ctx<'js>) -> Result<Promise<'js>> {
    let stdin = self.stdin.clone();
    promise(
      &ctx,
      io(async move {
        stdin.lock().await.take();
        Ok(())
      }),
    )
  }

  pub fn wait<'js>(&self, ctx: Ctx<'js>) -> Result<Promise<'js>> {
    let mut status = self.status.clone();
    promise(
      &ctx,
      io(async move {
        let status = status
          .wait_for(Option::is_some)
          .await
          .map_err(|_| Fail::Error("the child was lost".into()))?;
        Ok(Status(status.expect("waited for")))
      }),
    )
  }

  pub fn kill(&self, ctx: Ctx<'_>, name: Opt<String>) -> Result<()> {
    let name = name.0.unwrap_or_else(|| "SIGTERM".into());
    let Some(signal) = signal(&name) else {
      return Err(Exception::throw_type(
        &ctx,
        &format!("child.kill(signal) does not know `{name}`"),
      ));
    };
    if self.status.borrow().is_none() {
      unsafe { libc::kill(-(self.pid as i32), signal) };
    }
    Ok(())
  }
}

/// SIGTERM to the group, SIGKILL after the grace period if any of it is left
impl Drop for ChildProcess {
  fn drop(&mut self) {
    // a reaped child's pid may belong to someone else by now
    if self.status.borrow().is_some() {
      return;
    }
    let group = -(self.pid as i32);
    let status = self.status.clone();
    unsafe { libc::kill(group, libc::SIGTERM) };
    detach(async move {
      tokio::time::sleep(GRACE).await;
      if status.borrow().is_none() {
        unsafe { libc::kill(group, libc::SIGKILL) };
      }
    });
  }
}

fn spawn<'js>(
  ctx: Ctx<'js>,
  name: String,
  args_: Opt<Vec<String>>,
  opts: Opt<Object<'js>>,
) -> Result<ChildProcess> {
  let opts = options(&ctx, opts.0, true).map_err(|f| f.throw(&ctx))?;
  let mut cmd = command(&ctx, &name, args(args_), &opts).map_err(|f| f.throw(&ctx))?;
  let pipe = |on: bool| if on { Stdio::piped() } else { Stdio::null() };
  cmd
    .stdin(Stdio::piped())
    .stdout(pipe(opts.stdout))
    .stderr(pipe(opts.stderr));
  let _enter = enter();
  let mut child = cmd
    .spawn()
    .map_err(|e| Exception::throw_message(&ctx, &format!("starting `{name}` failed: {e}")))?;
  let pid = child.id().expect("a running child");
  runtime(&ctx)?.groups.add(pid);
  let (tx, status) = watch::channel(None);
  let process = ChildProcess {
    pid,
    stdout: Arc::new(Mutex::new(child.stdout.take().map(BufReader::new))),
    stderr: Arc::new(Mutex::new(child.stderr.take().map(BufReader::new))),
    stdin: Arc::new(Mutex::new(child.stdin.take())),
    status,
  };
  detach(async move {
    let status = child.wait().await;
    let _ = tx.send(Some(status.unwrap_or_else(|_| ExitStatus::from_raw(-1))));
  });
  Ok(process)
}

fn exit(ctx: Ctx<'_>) -> Result<()> {
  if !plugin(&ctx)?.capabilities.may_exit() {
    return Err(Exception::throw_type(
      &ctx,
      "exiting is not granted; set capabilities.process.exit to true",
    ));
  }
  app().quit();
  Ok(())
}

/// Node's name of the CPU architecture
fn arch() -> &'static str {
  match std::env::consts::ARCH {
    "x86_64" => "x64",
    "aarch64" => "arm64",
    "x86" => "ia32",
    other => other,
  }
}

/// The `process` object, also a global
pub(crate) fn object<'js>(ctx: &Ctx<'js>) -> Result<Object<'js>> {
  let o = Object::new(ctx.clone())?;
  o.set("run", Function::new(ctx.clone(), run)?)?;
  o.set("spawn", Function::new(ctx.clone(), spawn)?)?;
  o.set("exit", Function::new(ctx.clone(), exit)?)?;
  let next_tick: Function = ctx.eval("(f, ...args) => queueMicrotask(() => f(...args))")?;
  o.set("nextTick", next_tick)?;
  o.set("platform", std::env::consts::OS)?;
  o.set("arch", arch())?;
  Ok(o)
}

pub struct ProcessModule;

const NAMES: [&str; 6] = ["run", "spawn", "exit", "nextTick", "platform", "arch"];

impl ModuleDef for ProcessModule {
  fn declare<'js>(decl: &Declarations<'js>) -> Result<()> {
    for name in NAMES {
      decl.declare(name)?;
    }
    decl.declare("default")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> Result<()> {
    let process: Object = ctx.globals().get("process")?;
    for name in NAMES {
      exports.export(name, process.get::<_, Value>(name)?)?;
    }
    exports.export("default", process)?;
    Ok(())
  }
}

/// `os`
pub struct OsModule;

impl ModuleDef for OsModule {
  fn declare<'js>(decl: &Declarations<'js>) -> Result<()> {
    for name in ["platform", "arch", "EOL", "default"] {
      decl.declare(name)?;
    }
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> Result<()> {
    let o = Object::new(ctx.clone())?;
    o.set(
      "platform",
      Function::new(ctx.clone(), || std::env::consts::OS)?,
    )?;
    o.set("arch", Function::new(ctx.clone(), arch)?)?;
    o.set("EOL", "\n")?;
    for name in ["platform", "arch", "EOL"] {
      exports.export(name, o.get::<_, Value>(name)?)?;
    }
    exports.export("default", o)?;
    Ok(())
  }
}
