//! The stdlib and sandbox through real scripts: a service reports what it saw through
//! `corona/probe`.

use std::{
  io::{Read, Write},
  net::TcpListener,
  os::unix::net::UnixListener,
  path::{Path, PathBuf},
  rc::Rc,
  thread,
  time::{Duration, Instant},
};

use gpui_kit::{self as gpui, App, Global, TestAppContext};
use specta::specta;

use crate::{
  Capabilities, Cx, ExecuteGrant, ModuleBuilder, Plugin, PluginRef, ScriptExt, ScriptModule,
  ScriptRegistry, ScriptView, Service, Watches, reload_view,
};

#[derive(Default)]
struct Log(Vec<String>);

impl Global for Log {}

/// Appends `text` to the log.
#[specta]
fn report(mut cx: Cx, text: String) {
  cx.default_global::<Log>().0.push(text);
}

/// The id of the calling plugin.
#[specta]
fn whoami(plugin: Plugin) -> String {
  plugin.id.clone()
}

struct Probe;

impl ScriptModule for Probe {
  const NAME: &str = "probe";
  const DOC: &str = "Reports to the test.";

  fn module(_: &PluginRef, m: &mut ModuleBuilder, _: &mut App) {
    crate::host_module!(m, { sync: [report, whoami] });
  }
}

/// A fresh directory named after `name`
fn dir(name: &str) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("corona-js-{name}-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(&dir).unwrap();
  dir.canonicalize().unwrap()
}

fn plugin(id: &str, dir: &Path, capabilities: Capabilities) -> Rc<PluginRef> {
  Rc::new(PluginRef {
    id: id.into(),
    name: id.into(),
    dir: dir.to_path_buf(),
    data_dir: dir.join("data"),
    capabilities,
  })
}

fn modules(plugin: &PluginRef, watches: &Watches, cx: &mut App) -> Vec<ModuleBuilder> {
  cx.script_mut().register::<Probe>();
  let probe = cx.global::<ScriptRegistry>().get(Probe::NAME).unwrap();
  vec![probe.build(plugin, watches, cx)]
}

fn start_in(
  cx: &mut TestAppContext,
  plugin: Rc<PluginRef>,
  entry: &Path,
) -> anyhow::Result<Service> {
  // IO threads wake the runtime's driver
  cx.executor().allow_parking();
  cx.update(|cx| {
    let watches = Watches::default();
    let modules = modules(&plugin, &watches, cx);
    Service::start(entry, plugin, modules, watches, cx)
  })
}

/// Runs `source` as the service of plugin `name` with `caps`
fn start(
  cx: &mut TestAppContext,
  name: &str,
  caps: Capabilities,
  source: &str,
) -> (Service, PathBuf) {
  let dir = dir(name);
  std::fs::write(dir.join("service.js"), source).unwrap();
  let service = start_in(cx, plugin(name, &dir, caps), &dir.join("service.js")).unwrap();
  (service, dir)
}

fn log(cx: &mut TestAppContext) -> Vec<String> {
  cx.update(|cx| cx.default_global::<Log>().0.clone())
}

/// Pumps timers and IO until `done` holds or ten seconds pass
fn settle(cx: &mut TestAppContext, done: impl Fn(&[String]) -> bool) -> Vec<String> {
  let deadline = Instant::now() + Duration::from_secs(10);
  loop {
    cx.run_until_parked();
    let seen = log(cx);
    if done(&seen) || Instant::now() >= deadline {
      return seen;
    }
    thread::sleep(Duration::from_millis(5));
    cx.executor().advance_clock(Duration::from_millis(10));
  }
}

/// The first line reported
fn first(cx: &mut TestAppContext) -> String {
  settle(cx, |log| !log.is_empty())
    .first()
    .cloned()
    .unwrap_or_default()
}

fn count(log: &[String], prefix: &str) -> usize {
  log.iter().filter(|l| l.starts_with(prefix)).count()
}

fn alive(pid: i32) -> bool {
  std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| !stat.contains(") Z "))
}

fn sh() -> Capabilities {
  Capabilities::new().execute(ExecuteGrant::Allowed(vec!["/bin/sh".into()]))
}

/// Wraps `body` in a service main that reports what it throws as `rejected:<message>`
fn main(imports: &str, body: &str) -> String {
  format!(
    r#"import {{ report }} from "corona/probe";
{imports}
export default async function main(cx) {{
  try {{
{body}
  }} catch (error) {{
    report(`rejected:${{error.name}}:${{error.message}}`);
  }}
}}"#
  )
}

// --- fs

#[gpui::test]
fn fs_round_trips_through_promises(cx: &mut TestAppContext) {
  let root = dir("fs-root");
  let caps = Capabilities::new()
    .read_roots([root.clone()])
    .write_roots([root.clone()]);
  let source = main(
    r#"import * as fs from "fs/promises";"#,
    r#"
    const pending = fs.writeFile("notes.txt", "hello");
    report(`promise:${pending instanceof Promise}`);
    await pending;
    await fs.writeFile("bytes.bin", new Uint8Array([0, 255, 42]));
    await fs.mkdir("sub/deeper", { recursive: true });
    const text = await fs.readFile("notes.txt", "utf8");
    const bytes = await fs.readFile("bytes.bin");
    const names = await fs.readdir(".");
    const typed = await fs.readdir(".", { withFileTypes: true });
    const exists = await fs.exists("notes.txt");
    await fs.unlink("notes.txt");
    report(`${text}|${[...bytes]}|${names}|${typed.find((e) => e.name === "sub").isDirectory()}|${exists}|${await fs.exists("notes.txt")}`);"#,
  );
  let caps2 = caps.clone();
  std::fs::write(root.join("service.js"), &source).unwrap();
  let _service = start_in(cx, plugin("fs", &root, caps2), &root.join("service.js")).unwrap();
  let seen = settle(cx, |log| log.len() >= 2);
  assert_eq!(
    seen,
    [
      "promise:true",
      "hello|0,255,42|bytes.bin,notes.txt,service.js,sub|true|true|false"
    ]
  );
  assert!(caps.may_read(&root.join("bytes.bin")));
}

#[gpui::test]
fn fs_is_denied_by_default(cx: &mut TestAppContext) {
  let source = main(
    r#"import * as fs from "fs/promises";"#,
    r#"await fs.readFile("/etc/hostname", "utf8"); report("allowed");"#,
  );
  let (_service, _) = start(cx, "fs-denied", Capabilities::new(), &source);
  let line = first(cx);
  assert!(
    line.starts_with("rejected:TypeError:") && line.contains("capabilities.fs.read"),
    "{line}"
  );
}

#[gpui::test]
fn fs_does_not_follow_a_symlink_out_of_a_root(cx: &mut TestAppContext) {
  let base = dir("fs-link");
  let (root, outside) = (base.join("root"), base.join("outside"));
  std::fs::create_dir_all(&root).unwrap();
  std::fs::create_dir_all(&outside).unwrap();
  std::fs::write(outside.join("secret.txt"), "secret").unwrap();
  std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();
  let caps = Capabilities::new()
    .read_roots([root.clone()])
    .write_roots([root.clone()]);
  let source = main(
    r#"import * as fs from "fs/promises";"#,
    r#"
    for (const op of [() => fs.readFile("escape/secret.txt", "utf8"), () => fs.writeFile("escape/planted.txt", "x"), () => fs.readFile("../outside/secret.txt", "utf8")]) {
      try { report(`allowed:${await op()}`); } catch (error) { report("refused"); }
    }"#,
  );
  let (_service, _) = start(cx, "fs-link-plugin", caps, &source);
  assert_eq!(settle(cx, |log| log.len() >= 3), ["refused"; 3]);
  assert!(!outside.join("planted.txt").exists());
}

#[gpui::test]
fn fs_refuses_oversized_reads_and_writes(cx: &mut TestAppContext) {
  let root = dir("fs-big");
  let file = std::fs::File::create(root.join("big.bin")).unwrap();
  file.set_len(65 * 1024 * 1024).unwrap();
  let caps = Capabilities::new()
    .read_roots([root.clone()])
    .write_roots([root.clone()]);
  let source = main(
    r#"import * as fs from "fs/promises";"#,
    r#"
    try { await fs.readFile("big.bin"); } catch (error) { report(error.message); }
    try { await fs.writeFile("notes.txt", "x".repeat(8 * 1024 * 1024 + 1)); } catch (error) { report(error.message); }"#,
  );
  std::fs::write(root.join("service.js"), &source).unwrap();
  let _service = start_in(cx, plugin("fs-big", &root, caps), &root.join("service.js")).unwrap();
  let seen = settle(cx, |log| log.len() >= 2);
  assert!(
    seen[0].contains("big.bin") && seen[0].contains("limit"),
    "{seen:?}"
  );
  assert!(seen[1].contains("write limit"), "{seen:?}");
  assert!(!root.join("notes.txt").exists());
}

#[gpui::test]
fn file_handles_read_write_append_and_respect_the_grant(cx: &mut TestAppContext) {
  let root = dir("fs-handle");
  let caps = Capabilities::new()
    .read_roots([root.clone()])
    .write_roots([root.clone()]);
  let source = main(
    r#"import * as fs from "fs/promises";"#,
    r#"
    let denied = "allowed";
    try { await fs.open("/corona-outside/x", "r"); } catch (error) { denied = "denied"; }
    let bad = "allowed";
    try { await fs.open("log.txt", "x"); } catch (error) { bad = error.name; }
    const w = await fs.open("log.txt", "w");
    await w.write("ab");
    await w.write(new Uint8Array([99]));
    await w.close();
    let closed = "open";
    try { await w.write("late"); } catch (error) { closed = error.message; }
    const a = await fs.open("log.txt", "a");
    await a.write("d");
    await a.close();
    const r = await fs.open("log.txt");
    const head = await r.read(2);
    const rest = await r.read();
    const eof = await r.read();
    await r.close();
    report(`${denied}|${bad}|${closed}|${String.fromCharCode(...head)}|${String.fromCharCode(...rest)}|${eof}`);"#,
  );
  std::fs::write(root.join("service.js"), &source).unwrap();
  let _service = start_in(
    cx,
    plugin("fs-handle", &root, caps),
    &root.join("service.js"),
  )
  .unwrap();
  assert_eq!(first(cx), "denied|TypeError|the file is closed|ab|cd|null");
}

#[test]
fn a_picked_file_is_readable_and_nothing_beside_it() {
  let root = dir("picked");
  std::fs::write(root.join("picked.txt"), "x").unwrap();
  std::fs::write(root.join("beside.txt"), "x").unwrap();
  let caps = Capabilities::new();
  let clone = caps.clone();
  caps.grant_read(root.join("picked.txt"));
  assert!(clone.may_read(&root.join("picked.txt")));
  assert!(!clone.may_read(&root.join("beside.txt")));
  assert!(!clone.may_read(&root.join("picked.txt/..")));
  assert!(!clone.may_read(Path::new("../picked.txt")));
}

// --- storage

#[gpui::test]
fn storage_answers_from_memory_and_persists_atomically(cx: &mut TestAppContext) {
  let source = main(
    "",
    r#"
    localStorage.setItem("window", JSON.stringify({ title: "Notes" }));
    localStorage.setItem("theme", "dark");
    localStorage.setItem("scratch", "x");
    localStorage.removeItem("scratch");
    sessionStorage.setItem("session", "only");
    report(`${JSON.parse(localStorage.getItem("window")).title}|${localStorage.length}|${localStorage.key(0)}|${window.localStorage === localStorage}`);
    await localStorage.flush();
    report("flushed");"#,
  );
  let (service, dir) = start(cx, "store", Capabilities::new().storage(true), &source);
  assert_eq!(
    settle(cx, |log| log.len() >= 2),
    ["Notes|2|theme|true", "flushed"]
  );
  let file = dir.join("data/localStorage.json");
  let written = std::fs::read_to_string(&file).unwrap();
  assert!(
    written.contains("\"theme\":\"dark\"") && written.contains("Notes"),
    "{written}"
  );
  assert!(
    !written.contains("scratch") && !written.contains("session"),
    "{written}"
  );
  assert!(!dir.join("data/localStorage.json.tmp").exists());
  drop(service);

  // the next runtime of the plugin reads it back
  let again = main("", r#"report(localStorage.getItem("theme"));"#);
  std::fs::write(dir.join("again.js"), again).unwrap();
  cx.update(|cx| cx.remove_global::<Log>());
  let plugin = plugin("store", &dir, Capabilities::new().storage(true));
  let _service = start_in(cx, plugin, &dir.join("again.js")).unwrap();
  assert_eq!(first(cx), "dark");
}

#[gpui::test]
fn storage_is_denied_without_the_grant(cx: &mut TestAppContext) {
  let source = main("", r#"localStorage.getItem("x"); report("allowed");"#);
  let (_service, _) = start(cx, "store-denied", Capabilities::new(), &source);
  let line = first(cx);
  assert!(line.contains("TypeError:storage is not granted"), "{line}");
}

#[gpui::test]
fn a_failed_store_write_parks_until_flush_retries(cx: &mut TestAppContext) {
  let root = dir("store-retry");
  let file = root.join("data/localStorage.json");
  std::fs::create_dir_all(&file).unwrap();
  let source = main(
    "",
    r#"
    localStorage.setItem("attempt", "1");
    try { await localStorage.flush(); report("flushed"); } catch (error) { report("failed"); }
    await cx.sleep(1000);
    await localStorage.flush();
    report("retried");"#,
  );
  std::fs::write(root.join("service.js"), &source).unwrap();
  let plugin = plugin("store-retry", &root, Capabilities::new().storage(true));
  let _service = start_in(cx, plugin, &root.join("service.js")).unwrap();
  assert_eq!(first(cx), "failed");
  assert!(file.is_dir());
  std::fs::remove_dir(&file).unwrap();
  settle(cx, |log| log.len() >= 2);
  let written = std::fs::read_to_string(&file).unwrap();
  assert!(written.contains("\"attempt\":\"1\""), "{written}");
}

// --- process

#[gpui::test]
fn run_exposes_status_and_both_streams(cx: &mut TestAppContext) {
  let source = main(
    r#"import process from "process";"#,
    r#"const out = await process.run("/bin/sh", ["-c", "printf out; printf err >&2; exit 7"]);
    report(`${out.code}|${out.stdout}|${out.stderr}`);"#,
  );
  let (_service, _) = start(cx, "run", sh(), &source);
  assert_eq!(first(cx), "7|out|err");
}

#[gpui::test]
fn run_rejects_a_start_failure_and_bounds_output(cx: &mut TestAppContext) {
  let source = main(
    r#"import { run } from "process";"#,
    r#"
    try { await run("/corona-no-such-command"); } catch (error) { report(error.message); }
    try { await run("/bin/sh", ["-c", "yes x | head -c 8388609"]); } catch (error) { report(error.message); }"#,
  );
  let caps = Capabilities::new()
    .execute(ExecuteGrant::Allowed(vec![
      "/bin/sh".into(),
      "/corona-no-such-command".into(),
    ]))
    .process_env(["PATH".into()]);
  let (_service, _) = start(cx, "run-fail", caps, &source);
  let seen = settle(cx, |log| log.len() >= 2);
  assert!(
    seen[0].contains("/corona-no-such-command") && seen[0].contains("failed"),
    "{seen:?}"
  );
  assert!(seen[1].contains("stdout exceeded"), "{seen:?}");
}

#[gpui::test]
fn process_is_denied_and_ambient_authority_is_absent(cx: &mut TestAppContext) {
  let source = main(
    r#"import process from "process";"#,
    r#"
    try { process.run("/bin/sh"); report("allowed"); }
    catch (error) {
      report([error.message, typeof process.kill, typeof process.env, typeof globalThis.process.setuid].join("|"));
    }
    process.nextTick((suffix) => report(`tick${suffix}`), "!");"#,
  );
  let (_service, _) = start(cx, "process-denied", Capabilities::new(), &source);
  let seen = settle(cx, |log| log.len() >= 2);
  assert!(
    seen[0].contains("capabilities.fs.execute")
      && seen[0].ends_with("|undefined|undefined|undefined"),
    "{seen:?}"
  );
  assert_eq!(seen[1], "tick!");
}

#[gpui::test]
fn spawn_checks_command_env_and_cwd_against_the_grant(cx: &mut TestAppContext) {
  let cwd = std::env::temp_dir().canonicalize().unwrap();
  let source = main(
    r#"import { run, spawn } from "process";"#,
    &r#"
    const errors = [];
    for (const call of [
      () => spawn("/bin/sh", ["-c", "true"], { env: { LD_PRELOAD: "x" } }),
      () => spawn("/bin/sh", ["-c", "true"], { cwd: "relative" }),
      () => spawn("/corona-ungranted"),
    ]) {
      try { call(); errors.push("allowed"); } catch (error) { errors.push(`${error.name}:${error.message}`); }
    }
    const out = await run("/bin/sh", ["-c", 'printf "%s@%s" "$CORONA_PROBE" "$(pwd)"'], { env: { CORONA_PROBE: "set" }, cwd: "__CWD__" });
    report(`${errors.join("~")}~${out.stdout}`);"#
      .replace("__CWD__", &cwd.display().to_string()),
  );
  let caps = sh()
    .process_env(["CORONA_PROBE".into()])
    .read_roots([cwd.clone()]);
  let (_service, _) = start(cx, "spawn-policy", caps, &source);
  let line = first(cx);
  for expected in [
    "TypeError:environment variable `LD_PRELOAD` is not granted",
    "capabilities.process.env",
    "TypeError:process options.cwd `relative` must be an absolute path",
    "TypeError:running `/corona-ungranted` is not granted",
    &format!("set@{}", cwd.display()),
  ] {
    assert!(line.contains(expected), "{expected} missing from {line}");
  }
}

#[gpui::test]
fn spawn_round_trips_lines_stdin_and_exit_status(cx: &mut TestAppContext) {
  let source = main(
    r#"import { spawn } from "process";"#,
    r#"
    const child = spawn("/bin/sh", ["-c", "echo a; echo b >&2; read x; echo got:$x; exit 3"]);
    const a = await child.readLine();
    const b = await child.readErrLine();
    await child.write("hi\n");
    const got = await child.readLine();
    const eof = await child.readLine();
    const status = await child.wait();
    report(`${a}|${b}|${got}|${eof}|${status.code}|${status.signal}`);

    const ignored = spawn("/bin/sh", ["-c", "echo hi; read x"], { stdout: "ignore" });
    const line = await ignored.readLine();
    ignored.kill();
    const killed = await ignored.wait();
    report(`${line}|${killed.code}|${killed.signal}`);

    const cat = spawn("/bin/sh", ["-c", 'while read -r l; do echo "$l"; done; echo eof']);
    await cat.write("hi\n");
    await cat.closeStdin();
    let late = "allowed";
    try { await cat.write("late\n"); } catch (error) { late = error.message; }
    const lines = [await cat.readLine(), await cat.readLine(), await cat.readLine()];
    report(`${lines.map(String).join(",")}|${late}|${(await cat.wait()).code}`);"#,
  );
  let (_service, _) = start(cx, "spawn-trip", sh(), &source);
  assert_eq!(
    settle(cx, |log| log.len() >= 3),
    [
      "a|b|got:hi|null|3|null",
      "null|null|15",
      "hi,eof,null|the child's stdin is closed|0"
    ]
  );
}

#[gpui::test]
fn spawn_refuses_malformed_options_and_bounds_its_io(cx: &mut TestAppContext) {
  let source = main(
    r#"import { run, spawn } from "process";"#,
    r#"
    const errors = [];
    for (const call of [
      () => run("/bin/sh", ["-c", "true"], { stdout: "pipe" }),
      () => spawn("/bin/sh", ["-c", "true"], { shell: true }),
      () => spawn("/bin/sh", ["-c", "true"], { env: { HOME: 1 } }),
      () => spawn("/bin/sh", ["-c", "true"], { stderr: "inherit" }),
      () => spawn("/bin/sh", ["-c", "true"], { cwd: "/" }),
    ]) {
      try { call(); errors.push("allowed"); } catch (error) { errors.push(`${error.name}:${error.message}`); }
    }
    const child = spawn("/bin/sh", ["-c", "printf '%01048577d' 0"]);
    try { child.kill("SIGUSR1"); errors.push("allowed"); } catch (error) { errors.push(`${error.name}:${error.message}`); }
    try { child.write("x".repeat(1048577)); errors.push("allowed"); } catch (error) { errors.push(`${error.name}:${error.message}`); }
    try { await child.readLine(); errors.push("allowed"); } catch (error) { errors.push(`rejected:${error.message}`); }
    const status = await child.wait();
    try { await child.write("late\n"); errors.push("allowed"); } catch (error) { errors.push(`rejected:${error.message}`); }
    report(`${errors.join("~")}~${status.code}`);"#,
  );
  let (_service, _) = start(cx, "spawn-limits", sh(), &source);
  let line = first(cx);
  for expected in [
    "TypeError:process.run(command, args, options) captures both streams",
    "TypeError:unknown process option `shell`",
    "TypeError:process options.env expects string values",
    "TypeError:process options.stderr `inherit` must be",
    "TypeError:filesystem read is not granted",
    "TypeError:child.kill(signal) does not know `SIGUSR1`",
    "RangeError:child.write(data) exceeded the 1048576 byte limit",
    "rejected:a child output line exceeded 1048576 bytes",
    "rejected:writing to the child failed",
    "~0",
  ] {
    assert!(line.contains(expected), "{expected} missing from {line}");
  }
  assert!(!line.contains("allowed"), "{line}");
}

/// The pids a service reported as `pids:<a>:<b>`
fn pids(seen: &[String]) -> Vec<i32> {
  let line = seen
    .iter()
    .find_map(|l| l.strip_prefix("pids:"))
    .expect("pids");
  line.split(':').map(|p| p.trim().parse().unwrap()).collect()
}

#[gpui::test]
fn dropping_a_service_cancels_timers_and_kills_its_process_group(cx: &mut TestAppContext) {
  let source = main(
    r#"import { spawn } from "process";"#,
    r#"
    const child = (globalThis.child = spawn("/bin/sh", ["-c", "exec 3<&0; (read x <&3) & echo $!; read y"]));
    report(`pids:${child.pid}:${await child.readLine()}`);
    cx.timer.every(10, () => report("tick"));"#,
  );
  let (service, _) = start(cx, "drop-kill", sh(), &source);
  let seen = settle(cx, |log| count(log, "tick") >= 1);
  let pids = pids(&seen);
  assert!(pids.iter().all(|&p| alive(p)), "{seen:?}");

  drop(service);
  let ticks = count(&log(cx), "tick");
  let deadline = Instant::now() + Duration::from_secs(5);
  while pids.iter().any(|&p| alive(p)) {
    assert!(Instant::now() < deadline, "a process outlived its service");
    cx.executor().advance_clock(Duration::from_millis(10));
    cx.run_until_parked();
    thread::sleep(Duration::from_millis(5));
  }
  assert_eq!(
    count(&log(cx), "tick"),
    ticks,
    "a timer outlived its service"
  );
}

#[gpui::test]
fn dropping_a_service_ends_what_run_left_in_the_background(cx: &mut TestAppContext) {
  let source = main(
    r#"import { run } from "process";"#,
    r#"
    const out = await run("/bin/sh", ["-c", "sleep 100 </dev/null >/dev/null 2>&1 & echo $!"]);
    report(`pids:${out.stdout.trim()}`);"#,
  );
  let (service, _) = start(
    cx,
    "run-leftover",
    sh().process_env(["PATH".into()]),
    &source,
  );
  let seen = settle(cx, |log| !log.is_empty());
  let pid = pids(&seen)[0];
  assert!(alive(pid), "{seen:?}");
  drop(service);
  let deadline = Instant::now() + Duration::from_secs(5);
  while alive(pid) {
    assert!(
      Instant::now() < deadline,
      "a background process outlived its service"
    );
    thread::sleep(Duration::from_millis(20));
  }
}

#[gpui::test]
fn a_child_that_ignores_sigterm_gets_sigkill_after_the_grace_period(cx: &mut TestAppContext) {
  let source = main(
    r#"import { spawn } from "process";"#,
    r#"
    globalThis.child = spawn("/bin/sh", ["-c", "trap '' TERM; echo started; while :; do read x; done"]);
    await child.readLine();
    report(`pids:${child.pid}`);"#,
  );
  let (service, _) = start(cx, "drop-term", sh(), &source);
  let pid = pids(&settle(cx, |log| !log.is_empty()))[0];
  drop(service);
  let started = Instant::now();
  let mut survived = false;
  while alive(pid) {
    assert!(
      started.elapsed() < Duration::from_secs(6),
      "SIGKILL never came"
    );
    survived |= started.elapsed() > Duration::from_secs(1);
    thread::sleep(Duration::from_millis(20));
  }
  assert!(survived, "the child should have ignored SIGTERM");
}

// --- network

/// Answers one HTTP request on a local port with `response`
fn serve_once(response: impl Into<Vec<u8>> + Send + 'static) -> std::net::SocketAddr {
  let listener = TcpListener::bind("127.0.0.1:0").unwrap();
  let addr = listener.local_addr().unwrap();
  thread::spawn(move || {
    let (mut stream, _) = listener.accept().unwrap();
    let mut request = [0; 4096];
    let _ = stream.read(&mut request);
    let _ = stream.write_all(&response.into());
  });
  addr
}

fn local() -> Capabilities {
  Capabilities::new().network_hosts(["127.0.0.1".into()])
}

#[gpui::test]
fn network_is_denied_by_default(cx: &mut TestAppContext) {
  let source = main(
    r#"import { connect } from "net"; import { WebSocket } from "websocket";"#,
    r#"
    for (const op of [() => fetch("http://127.0.0.1:9/"), () => connect("127.0.0.1", 9), () => WebSocket.connect("ws://127.0.0.1:9/")]) {
      try { await op(); report("allowed"); } catch (error) { report(`${error.name}:${error.message}`); }
    }"#,
  );
  let (_service, _) = start(cx, "net-denied", Capabilities::new(), &source);
  let seen = settle(cx, |log| log.len() >= 3);
  assert!(
    seen[0].contains("TypeError:HTTP request GET") && seen[0].contains("not granted"),
    "{seen:?}"
  );
  assert!(
    seen[1..]
      .iter()
      .all(|l| l.starts_with("TypeError:") && l.contains("not granted")),
    "{seen:?}"
  );
}

#[gpui::test]
fn fetch_reads_a_response_and_refuses_one_over_the_limit(cx: &mut TestAppContext) {
  let ok = serve_once(
    "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nX-A: 1\r\nX-A: 2\r\nContent-Length: 13\r\nConnection: close\r\n\r\n{\"ok\": true }",
  );
  let big = 8 * 1024 * 1024 + 1;
  let mut large =
    format!("HTTP/1.1 200 OK\r\nContent-Length: {big}\r\nConnection: close\r\n\r\n").into_bytes();
  large.extend(vec![b'x'; big]);
  let large = serve_once(large);
  let source = main(
    "",
    &r#"
    const response = await fetch("http://OK/path", { method: "post", body: "data", headers: { "X-Test": "1" } });
    const json = await response.json();
    report(`${response.status}|${response.ok}|${response.headers["x-a"]}|${json.ok}`);
    try { await fetch("http://LARGE/"); report("allowed"); } catch (error) { report(error.message); }
    try { await fetch("http://OK/", { headers: { Host: "x" } }); } catch (error) { report(error.message); }"#
      .replace("OK", &ok.to_string())
      .replace("LARGE", &large.to_string()),
  );
  let (_service, _) = start(cx, "fetch", local(), &source);
  let seen = settle(cx, |log| log.len() >= 3);
  assert_eq!(seen[0], "200|true|1, 2|true");
  assert!(seen[1].contains("8388608 byte limit"), "{seen:?}");
  assert!(seen[2].contains("may not set `Host`"), "{seen:?}");
}

#[gpui::test]
fn fetch_refuses_method_override_headers(cx: &mut TestAppContext) {
  let source = main(
    "",
    &r#"
    for (const name of ["X-HTTP-Method-Override", "x-http-method", "X-Method-Override"]) {
      try { await fetch("http://OK/", { headers: { [name]: "DELETE" } }); } catch (error) { report(error.message); }
    }"#
      .replace("OK", &serve_once("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").to_string()),
  );
  let (_service, _) = start(cx, "fetch-override", local(), &source);
  let seen = settle(cx, |log| log.len() >= 3);
  assert_eq!(seen.len(), 3, "{seen:?}");
  assert!(seen.iter().all(|m| m.contains("may not set")), "{seen:?}");
}

#[gpui::test]
fn fetch_https_does_not_panic_on_crypto_provider(cx: &mut TestAppContext) {
  let source = main(
    "",
    r#"
    try {
      await fetch("https://127.0.0.1:9/");
      report("allowed");
    } catch (error) {
      report(error.message);
    }"#,
  );
  let (_service, _) = start(cx, "fetch_https", local(), &source);
  let seen = settle(cx, |log| !log.is_empty());
  assert!(!seen[0].contains("panicked"), "{seen:?}");
}

#[gpui::test]
fn fetch_reauthorizes_every_redirect_target(cx: &mut TestAppContext) {
  let target = serve_once("HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
  let redirect = serve_once(format!(
    "HTTP/1.1 302 Found\r\nLocation: http://localhost:{}/denied\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    target.port()
  ));
  let source = main(
    "",
    &format!(r#"await fetch("http://{redirect}/"); report("allowed");"#),
  );
  let (_service, _) = start(cx, "redirect", local(), &source);
  let line = first(cx);
  assert!(
    line.contains("redirect target refused") && line.contains("localhost"),
    "{line}"
  );
}

#[gpui::test]
fn two_runtimes_keep_distinct_network_policies(cx: &mut TestAppContext) {
  let addr = serve_once("HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
  let source = main(
    "",
    &format!(r#"const r = await fetch("http://{addr}/"); report(`${{await r.text()}}`);"#),
  );
  let (_denied, _) = start(cx, "policy-denied", Capabilities::new(), &source);
  let (_granted, _) = start(cx, "policy-granted", local(), &source);
  let mut seen = settle(cx, |log| log.len() >= 2);
  seen.sort();
  assert_eq!(seen[0], "ok");
  assert!(seen[1].contains("not granted"), "{seen:?}");
}

fn socket(name: &str) -> PathBuf {
  dir(name).join("s.sock")
}

#[gpui::test]
fn fetch_and_net_reach_only_granted_unix_sockets(cx: &mut TestAppContext) {
  let http = socket("unix-http");
  let listener = UnixListener::bind(&http).unwrap();
  thread::spawn(move || {
    let (mut stream, _) = listener.accept().unwrap();
    let mut request = [0; 4096];
    let n = stream.read(&mut request).unwrap();
    let request = String::from_utf8_lossy(&request[..n]).into_owned();
    let line = request.lines().next().unwrap_or_default().to_string();
    let body = format!("{line}|{}", request.contains("host: local-tailscaled.sock"));
    let response = format!(
      "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{body}\r\n0\r\n\r\n",
      body.len()
    );
    stream.write_all(response.as_bytes()).unwrap();
  });
  let echo = socket("unix-echo");
  let listener = UnixListener::bind(&echo).unwrap();
  thread::spawn(move || {
    let (mut stream, _) = listener.accept().unwrap();
    let mut bytes = [0; 3];
    stream.read_exact(&mut bytes).unwrap();
    thread::sleep(Duration::from_millis(100));
    stream.write_all(&bytes).unwrap();
  });
  // a link planted under a granted name resolves elsewhere
  let other = socket("unix-other");
  let _other = UnixListener::bind(&other).unwrap();
  let link = echo.with_file_name("link.sock");
  std::os::unix::fs::symlink(&other, &link).unwrap();
  let source = main(
    r#"import { connect } from "net";"#,
    &r#"
    const stream = await fetch("http://local-tailscaled.sock/localapi/v0/status?x=1", { unix: "HTTP", stream: true });
    let text = "";
    for (let chunk; (chunk = await stream.read()); ) text += String.fromCharCode(...chunk);
    report(`${stream.status}|${text}`);
    const socket = await connect({ path: "ECHO" });
    await socket.write(new Uint8Array([0, 255, 128]));
    report(`bytes:${[...(await socket.read(3))]}`);
    socket.close();
    report(`closed:${await socket.read()}`);
    for (const path of ["LINK", "/run/corona-not-granted.sock"]) {
      try { await connect({ path }); report("allowed"); } catch (error) { report(error.message); }
    }
    try { await fetch("http://x/", { unix: "/run/corona-not-granted.sock" }); } catch (error) { report(error.message); }"#
      .replace("HTTP", &http.display().to_string())
      .replace("ECHO", &echo.display().to_string())
      .replace("LINK", &link.display().to_string()),
  );
  let caps = Capabilities::new().network_unix([http.clone(), echo.clone(), link.clone()]);
  let (_service, _) = start(cx, "unix", caps, &source);
  let seen = settle(cx, |log| log.len() >= 6);
  assert_eq!(seen[0], "200|GET /localapi/v0/status?x=1 HTTP/1.1|true");
  assert_eq!(seen[1..3], ["bytes:0,255,128", "closed:null"]);
  assert!(
    seen[3..].iter().all(|l| l.contains("is not granted")),
    "{seen:?}"
  );
}

#[gpui::test]
fn websocket_round_trips_messages(cx: &mut TestAppContext) {
  let listener = TcpListener::bind("127.0.0.1:0").unwrap();
  let addr = listener.local_addr().unwrap();
  thread::spawn(move || {
    let (stream, _) = listener.accept().unwrap();
    let mut ws = tokio_tungstenite::tungstenite::accept(stream).unwrap();
    for _ in 0..2 {
      let message = ws.read().unwrap();
      ws.send(message).unwrap();
    }
  });
  let source = main(
    r#"import { WebSocket } from "websocket";"#,
    &format!(
      r#"
    try {{ await WebSocket.connect("ws://{addr}/", {{ headers: {{ Cookie: "x" }} }}); }} catch (error) {{ report(error.message); }}
    const socket = await WebSocket.connect("ws://{addr}/");
    await socket.write("hi");
    await socket.write(new Uint8Array([1, 2]));
    report(`${{await socket.read()}}|${{[...(await socket.read())]}}`);
    await socket.close();"#
    ),
  );
  let (_service, _) = start(cx, "ws", local(), &source);
  let seen = settle(cx, |log| log.len() >= 2);
  assert!(seen[0].contains("may not set `Cookie`"), "{seen:?}");
  assert_eq!(seen[1], "hi|1,2");
}

// --- services and timers

#[gpui::test]
fn main_runs_and_its_sleeps_and_timers_resume(cx: &mut TestAppContext) {
  let source = main(
    "",
    r#"
    report("started");
    await cx.sleep(20);
    report("slept");
    let ticks = 0;
    const task = cx.timer.every(10, () => report(`tick:${++ticks}`));
    const cancelled = cx.timer.after(10, () => report("cancelled timer ran"));
    cancelled.cancel();
    const id = setTimeout(() => report("cleared timeout ran"), 5);
    clearTimeout(id);
    setTimeout((a) => report(`timeout:${a}`), 5, "arg");
    report(`done:${task.is_done()}|${cancelled.is_done()}`);"#,
  );
  let (_service, _) = start(cx, "service-timers", Capabilities::new(), &source);
  let seen = settle(cx, |log| {
    count(log, "tick:") >= 2 && count(log, "timeout:") >= 1
  });
  assert_eq!(
    seen[..3],
    ["started", "slept", "done:false|true"],
    "{seen:?}"
  );
  assert!(seen.contains(&"timeout:arg".to_string()), "{seen:?}");
  assert!(!seen.iter().any(|l| l.contains("ran")), "{seen:?}");
}

#[gpui::test]
fn search_params_split_on_the_first_equals_and_survive_bad_escapes(cx: &mut TestAppContext) {
  let source = main(
    "",
    r#"
    const p = new URLSearchParams("a=b=c&bad=%zz&sp=x+y");
    p.append("q", "1 2");
    report(`${p.get("a")}|${p.get("bad")}|${p.get("sp")}|${p.toString()}`);"#,
  );
  let (_service, _) = start(cx, "search-params", Capabilities::new(), &source);
  assert_eq!(first(cx), "b=c|%zz|x y|a=b%3Dc&bad=%25zz&sp=x+y&q=1+2");
}

#[gpui::test]
fn the_plugin_param_names_the_caller(cx: &mut TestAppContext) {
  let source = r#"import { report, whoami } from "corona/probe";
export default function main() { report(whoami()); }"#;
  let (_service, _) = start(cx, "whoami", Capabilities::new(), source);
  assert_eq!(first(cx), "whoami");
}

#[gpui::test]
fn a_default_export_that_is_not_a_function_is_refused(cx: &mut TestAppContext) {
  let root = dir("service-object");
  std::fs::write(root.join("service.js"), "export default { main() {} };").unwrap();
  let error = start_in(
    cx,
    plugin("object", &root, Capabilities::new()),
    &root.join("service.js"),
  );
  let error = format!("{:#}", error.err().expect("refused"));
  assert!(
    error.contains("must `export default` a function"),
    "{error}"
  );
}

#[gpui::test]
fn a_failing_main_is_logged_not_fatal(cx: &mut TestAppContext) {
  let (_thrown, _) = start(
    cx,
    "thrown",
    Capabilities::new(),
    r#"export default function main() { throw new Error("boom"); }"#,
  );
  let (_rejected, _) = start(
    cx,
    "rejected",
    Capabilities::new(),
    r#"export default async function main(cx) { await cx.sleep(1); throw new Error("boom"); }"#,
  );
  cx.executor().advance_clock(Duration::from_millis(10));
  cx.run_until_parked();
  let (_service, _) = start(
    cx,
    "after-failure",
    Capabilities::new(),
    r#"import { report } from "corona/probe"; export default function main() { report("ok"); }"#,
  );
  assert_eq!(first(cx), "ok");
}

/// Writes `source` to `path` with a later modification time, then lets the watcher see it
fn rewrite(cx: &mut TestAppContext, path: &Path, source: &str) {
  thread::sleep(Duration::from_millis(20));
  std::fs::write(path, source).unwrap();
  for _ in 0..2 {
    cx.executor().advance_clock(crate::POLL * 2);
    cx.run_until_parked();
  }
}

fn ticker(name: &str) -> String {
  format!(
    r#"import {{ report }} from "corona/probe";
export default function main(cx: {{ timer: {{ every(ms: number, f: () => void): void }} }}) {{
  cx.timer.every(10, () => report("{name}" as string));
}}
"#
  )
}

/// Watches `entry` of plugin `id` as its service
fn watched(cx: &mut TestAppContext, id: &str, entry: &Path) -> gpui::Task<()> {
  let dir = entry.parent().unwrap().to_path_buf();
  let (plugin, entry) = (plugin(id, &dir, Capabilities::new()), entry.to_path_buf());
  cx.update(|cx| {
    Service::watch(
      &dir,
      move |cx| {
        let watches = Watches::default();
        let modules = modules(&plugin, &watches, cx);
        Service::start(&entry, plugin.clone(), modules, watches, cx)
      },
      cx,
    )
  })
}

#[gpui::test]
fn a_typescript_service_restarts_on_save(cx: &mut TestAppContext) {
  let entry = dir("service-reload").join("service.ts");
  std::fs::write(&entry, ticker("old")).unwrap();
  let watcher = watched(cx, "reload", &entry);
  settle(cx, |log| count(log, "old") >= 1);

  rewrite(cx, &entry, &ticker("new"));
  settle(cx, |log| count(log, "new") >= 1);
  let old = count(&log(cx), "old");
  let seen = settle(cx, |log| count(log, "new") >= 3);
  assert_eq!(count(&seen, "old"), old, "the old instance kept ticking");

  drop(watcher);
  let new = count(&log(cx), "new");
  cx.executor().advance_clock(Duration::from_millis(100));
  cx.run_until_parked();
  assert_eq!(
    count(&log(cx), "new"),
    new,
    "the service outlived its watcher"
  );
}

#[gpui::test]
fn a_service_that_fails_to_start_starts_once_fixed(cx: &mut TestAppContext) {
  let entry = dir("service-broken").join("service.js");
  std::fs::write(&entry, "export default 1;").unwrap();
  let _watcher = watched(cx, "broken", &entry);
  rewrite(
    cx,
    &entry,
    r#"import { report } from "corona/probe"; export default function main() { report("fixed"); }"#,
  );
  assert_eq!(first(cx), "fixed");
}

// --- typescript and files

/// The error of starting `files`' `main.ts` as a service
fn start_error(cx: &mut TestAppContext, name: &str, files: &[(&str, &str)]) -> String {
  let root = dir(name);
  for (file, source) in files {
    std::fs::write(root.join(file), source).unwrap();
  }
  let started = start_in(
    cx,
    plugin(name, &root, Capabilities::new()),
    &root.join("main.ts"),
  );
  format!("{:#}", started.err().expect("fails"))
}

#[gpui::test]
fn imports_reach_typescript_and_javascript_wins_a_tie(cx: &mut TestAppContext) {
  let root = dir("ts-imports");
  for (file, source) in [
    (
      "main.ts",
      "import { report } from 'corona/probe';\n\
       import { a } from './util';\n\
       import { a as b } from './util.js';\n\
       import { c } from './bridge.js';\n\
       import { d } from './both';\n\
       interface Unused { x: number }\n\
       export default function main(): void { report([a, b, c, d].join('/')); }\n",
    ),
    ("util.ts", "export const a: string = 'ts';"),
    ("bridge.js", "export { a as c } from './util.ts';"),
    ("both.js", "export const d = 'js';"),
    ("both.ts", "export const d: string = 'ts';"),
  ] {
    std::fs::write(root.join(file), source).unwrap();
  }
  let _service = start_in(
    cx,
    plugin("ts", &root, Capabilities::new()),
    &root.join("main.ts"),
  )
  .unwrap();
  assert_eq!(first(cx), "ts/ts/ts/js");
}

#[gpui::test]
fn imports_stay_inside_the_plugin(cx: &mut TestAppContext) {
  let outside = dir("ts-outside");
  std::fs::write(outside.join("secret.js"), "export const s = 1;").unwrap();
  assert!(outside.join("secret.js").exists());
  let main = format!(
    "import {{ s }} from '../corona-js-ts-outside-{}/secret.js';\nexport default function main() {{}}\n",
    std::process::id()
  );
  let error = start_error(cx, "ts-escape", &[("main.ts", &main)]);
  assert!(error.contains("secret.js"), "{error}");
}

#[gpui::test]
fn a_typescript_view_loads_and_reloads(cx: &mut TestAppContext) {
  let root = dir("view-reload");
  let entry = root.join("main.ts");
  let source = |text: &str| {
    format!(
      "import {{ View, div }} from 'corona/kit';\n\
       export default class Main extends View {{ render(_cx: unknown) {{ return div().child('{text}' as string); }} }}\n"
    )
  };
  std::fs::write(&entry, source("v1")).unwrap();
  let plugin = plugin("view", &root, Capabilities::new());
  let make = {
    let (entry, plugin) = (entry.clone(), plugin.clone());
    move |window: &mut gpui::Window, cx: &mut gpui::Context<ScriptView>| {
      ScriptView::load(
        &entry,
        plugin.clone(),
        Vec::new(),
        Watches::default(),
        window,
        cx,
      )
    }
  };
  let first = make.clone();
  let (view, vcx) = cx.add_window_view(move |window, cx| first(window, cx).unwrap());
  vcx.run_until_parked();
  assert_eq!(view.read_with(vcx, |v, _| v.text.clone()), "v1");

  let handle = vcx.update(|window, _| window.window_handle());
  let _task = vcx.update(|_, cx| reload_view(&view, handle, root.clone(), make, cx));
  std::thread::sleep(Duration::from_millis(20));
  std::fs::write(&entry, source("v2")).unwrap();
  for _ in 0..2 {
    vcx.executor().advance_clock(crate::POLL * 2);
    vcx.run_until_parked();
  }
  assert_eq!(view.read_with(vcx, |v, _| v.text.clone()), "v2");
}

#[gpui::test]
fn an_error_names_the_typescript_line_and_column(cx: &mut TestAppContext) {
  let error = start_error(
    cx,
    "ts-line",
    &[(
      "main.ts",
      "type Pair = [\n  number,\n  number,\n];\nconst pair: Pair = [1, 2];\n\
       const sum = (p: Pair): number => p[0] + p[1];\n\
       if (sum(pair) as number) throw new Error('boom');\n",
    )],
  );
  assert!(error.contains("main.ts:7:"), "{error}");

  let line = "let s: \"é\" | \"𝒳\" = \"é\"; throw new Error(s);";
  let error = start_error(cx, "ts-column", &[("main.ts", &format!("\n{line}"))]);
  // QuickJS counts bytes, and points at the argument of the throwing call
  let column = line.find("(s)").unwrap() + 2;
  assert!(error.contains(&format!("main.ts:2:{column}")), "{error}");

  let error = start_error(cx, "ts-enum", &[("main.ts", "\nenum Color { Red }\n")]);
  assert!(error.contains("main.ts:2:1:"), "{error}");
}
