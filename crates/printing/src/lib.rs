//! CUPS printers and print jobs, network printers and eSCL scanners found
//! through Avahi, and SANE scanners.

use std::{
  path::{Path, PathBuf},
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
  time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use gpui_kit::{App, AppContext, AsyncApp, Entity, Global, Task};
use zbus::Connection;

use corona_js::ScriptExt;
use corona_utils::{Buses, error::ErrorLogExt};

use crate::ipp::IppError;

pub use crate::state::{
  ColorMode, Format, InputSource, Job, JobState, Marker, NetworkPrinter, Printer, PrinterState,
  ScanProgress, ScanSettings, Scanner, ScannerCaps, SourceCaps,
};

mod avahi;
mod cups;
mod escl;
mod ipp;
mod listener;
#[cfg(test)]
mod mock;
mod sane;
pub mod script;
mod settings;
mod state;
mod tls;
pub mod ui;

rust_i18n::i18n!("../../assets/locales", fallback = "en");

#[derive(Clone)]
pub struct Printing {
  pub printers: Entity<Vec<Printer>>,
  pub jobs: Entity<Vec<Job>>,
  pub scanners: Entity<Vec<Scanner>>,
  /// every IPP printer announced on the network, with a queue or not
  pub network_printers: Entity<Vec<NetworkPrinter>>,
  /// the running scan, if any
  pub scan: Entity<Option<ScanProgress>>,
  sock: Arc<Path>,
  /// the system bus, for asking Avahi and cups-pk-helper
  system: Option<Connection>,
  /// pinned certificates of TLS scanners
  pins: Arc<Path>,
  /// asks the listener to read printers and jobs again
  poke: flume::Sender<()>,
  /// whether to ask SANE for scanners, never in tests
  sane: bool,
  /// a SANE probe is running
  probing: Arc<AtomicBool>,
}

impl Global for Printing {}

corona_utils::global_ext!(PrintingExt, printing, Printing);

impl Printing {
  pub fn list_printers<'c>(&self, cx: &'c App) -> &'c [Printer] {
    self.printers.read(cx)
  }

  pub fn list_jobs<'c>(&self, cx: &'c App) -> &'c [Job] {
    self.jobs.read(cx)
  }

  /// Network printers CUPS has no queue for yet.
  pub fn new_printers<'c>(&self, cx: &'c App) -> Vec<&'c NetworkPrinter> {
    let queues = self.list_printers(cx);
    let found = self.network_printers.read(cx);
    found
      .iter()
      .filter(|p| !avahi::configured(p, queues))
      .collect()
  }

  /// Makes a driverless queue for the network printer `name`, asking for an
  /// administrator when CUPS wants one.
  pub fn add_printer(&self, name: &str, cx: &App) -> impl Future<Output = Result<()>> + use<> {
    let printer = self
      .network_printers
      .read(cx)
      .iter()
      .find(|p| p.name == name)
      .cloned()
      .with_context(|| format!("no network printer {name}"));
    let queue = printer
      .as_ref()
      .ok()
      .map(|p| avahi::queue_name(p, self.list_printers(cx)));
    let (sock, system) = (self.sock.clone(), self.system.clone());
    self.after(async move {
      let (printer, queue) = (printer?, queue.unwrap_or_default());
      let info = printer
        .model
        .clone()
        .unwrap_or_else(|| printer.name.clone());
      // the address now: one announced earlier may be stale, e.g. from before DHCP
      let direct = async || {
        match &system {
          Some(conn) => avahi::resolve_printer(conn, &printer).await.log_err().ok(),
          None => None,
        }
        .unwrap_or_else(|| printer.direct_uri.clone())
      };

      // anything but a clear refusal from CUPS is worth another try, unless
      // the password prompt or pk-helper failed
      let retry = |e: &anyhow::Error| {
        e.downcast_ref::<AdminRefused>().is_none()
          && (cups::device_error(e) || e.downcast_ref::<IppError>().is_none())
      };
      let attempt = async |uri: &str, ppd: &str| match cups::add_printer(&sock, &queue, uri, ppd, &info).await {
        Err(e) if unauthorized(&e) => {
          let add = (queue.clone(), uri.to_string(), ppd.to_string(), info.clone(), "");
          // plain text errors, so never retried: that would ask for the password again
          async {
            pk_helper(system.as_ref(), "PrinterAdd", &add).await?;
            pk_helper(system.as_ref(), "PrinterSetEnabled", &(&queue, true)).await?;
            pk_helper(system.as_ref(), "PrinterSetAcceptJobs", &(&queue, true, "")).await
          }
          .await
          .map_err(|e| anyhow::Error::new(AdminRefused(format!("{e:#}"))))
        }
        result => result,
      };

      let mut uri = printer.uri.clone();
      let mut added = attempt(&uri, cups::DRIVERLESS).await;
      if added.as_ref().is_err_and(retry) {
        uri = direct().await;
        tracing::info!("cupsd cannot use {}, adding {uri} instead", printer.uri);
        added = attempt(&uri, cups::DRIVERLESS).await;
      }
      // older printers announce IPP without supporting IPP Everywhere
      if let Err(e) = &added
        && retry(e)
      {
        let model = printer.model.as_deref().unwrap_or(&printer.name);
        added = match cups::find_driver(&sock, model).await {
          Ok(Some(ppd)) => {
            // such printers often fail Create-Job + Send-Document; IPP/1.0
            // has CUPS send the whole job with Print-Job instead
            if uri.starts_with("ipp") && !uri.contains('?') {
              uri.push_str("?version=1.0");
            }
            tracing::info!("{model} is not driverless, using the driver {ppd} at {uri}");
            attempt(&uri, &ppd).await
          }
          Ok(None) => Err(anyhow!(
            "{e}\n{model} cannot print without a driver, and CUPS has none for it. Install one, then add it again."
          )),
          Err(fe) => Err(anyhow!("{e}\nlooking for a driver for {model} failed: {fe:#}")),
        };
      }
      added
    })
  }

  pub fn list_scanners<'c>(&self, cx: &'c App) -> &'c [Scanner] {
    self.scanners.read(cx)
  }

  pub fn scan_progress<'c>(&self, cx: &'c App) -> Option<&'c ScanProgress> {
    self.scan.read(cx).as_ref()
  }

  /// Whether a print job or a scan is running.
  pub fn busy(&self, cx: &App) -> bool {
    self.scan.read(cx).is_some() || self.jobs.read(cx).iter().any(|j| j.state.is_active())
  }

  /// Reads printers and jobs again, for when the panel opens.
  pub fn refresh(&self) {
    self.poke.send(()).ok();
  }

  /// Asks SANE for scanners without eSCL in the background. It probes USB
  /// and the network for seconds, so only when the panel opens.
  pub fn find_sane_scanners(&self, cx: &mut App) {
    // a probe could disturb the device that is scanning
    if !self.sane || self.scan.read(cx).is_some() || self.probing.swap(true, Ordering::Relaxed) {
      return;
    }
    let (scanners, scan, probing) = (
      self.scanners.clone(),
      self.scan.clone(),
      self.probing.clone(),
    );
    cx.spawn(async move |cx| {
      let found = within(PROBE_TIMEOUT, sane::scanners()).await;
      probing.store(false, Ordering::Relaxed);
      cx.update(|cx| match found {
        // a scan that started meanwhile keeps its device busy, so unlisted
        Ok(_) if scan.read(cx).is_some() => {}
        Ok(found) => set_scanners(&scanners, cx, true, found),
        // keep what the last probe found
        Err(e) => tracing::debug!("no SANE scanners: {e:?}"),
      });
    })
    .detach();
  }

  fn after<F: Future<Output = Result<()>>>(
    &self,
    action: F,
  ) -> impl Future<Output = Result<()>> + use<F> {
    let poke = self.poke.clone();
    async move {
      let result = action.await;
      // without the D-Bus notifier nothing else would show the change
      poke.send(()).ok();
      result
    }
  }

  pub fn cancel_job(&self, id: i32) -> impl Future<Output = Result<()>> + use<> {
    let (sock, system) = (self.sock.clone(), self.system.clone());
    self.after(async move {
      match cups::cancel_job(&sock, id).await {
        Err(e) if unauthorized(&e) => {
          pk_helper(system.as_ref(), "JobCancelPurge", &(id, false)).await
        }
        result => result,
      }
    })
  }

  pub fn set_paused(
    &self,
    printer: &str,
    paused: bool,
  ) -> impl Future<Output = Result<()>> + use<> {
    let (sock, system, printer) = (self.sock.clone(), self.system.clone(), printer.to_string());
    self.after(async move {
      match cups::set_paused(&sock, &printer, paused).await {
        Err(e) if unauthorized(&e) => {
          pk_helper(system.as_ref(), "PrinterSetEnabled", &(printer, !paused)).await
        }
        result => result,
      }
    })
  }

  pub fn print_file(
    &self,
    printer: &str,
    path: PathBuf,
  ) -> impl Future<Output = Result<()>> + use<> {
    let (sock, printer) = (self.sock.clone(), printer.to_string());
    self.after(async move {
      let meta = smol::fs::metadata(&path)
        .await
        .with_context(|| format!("reading {}", path.display()))?;
      // a FIFO or device never ends, a huge file would be copied again
      if !meta.is_file() || meta.len() > MAX_PRINT_FILE {
        bail!("{} is not a regular file under 256 MiB", path.display());
      }
      let data = smol::fs::read(&path)
        .await
        .with_context(|| format!("reading {}", path.display()))?;
      let title = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
      cups::print(&sock, &printer, &title, data).await.map(drop)
    })
  }

  /// HTTPS with a pinned certificate for a TLS scanner, the app's client else.
  fn client(&self, scanner: &Scanner, cx: &App) -> Arc<dyn gpui_kit::http_client::HttpClient> {
    match scanner.tls {
      true => Arc::new(tls::PinnedClient::new(&scanner.name, &self.pins)),
      false => cx.http_client(),
    }
  }

  fn scanner(&self, name: &str, cx: &App) -> Result<Scanner> {
    self
      .list_scanners(cx)
      .iter()
      .find(|s| s.name == name)
      .cloned()
      .with_context(|| format!("no scanner {name}"))
  }

  /// What the scanner can do and its current state.
  pub fn scanner_capabilities(
    &self,
    name: &str,
    cx: &App,
  ) -> impl Future<Output = Result<ScannerCaps>> + use<> {
    let scanner = self.scanner(name, cx);
    let client = scanner.as_ref().ok().map(|s| self.client(s, cx));
    let scanning = self.scan.read(cx).is_some();
    let probing = self.probing.clone();
    async move {
      let scanner = scanner?;
      // probing a busy SANE device can fail
      if scanner.sane.is_some() {
        if scanning {
          return Err(anyhow!("a scan is running"));
        }
        if probing.load(Ordering::Relaxed) {
          return Err(anyhow!("scanner search running"));
        }
      }
      within(PROBE_TIMEOUT, async {
        match &scanner.sane {
          Some(device) => sane::capabilities(device).await,
          None => escl::capabilities(&*client.context("no client")?, &scanner).await,
        }
      })
      .await
    }
  }

  /// Scans into `settings.destination` and returns the saved files.
  pub fn scan(
    &self,
    name: &str,
    settings: ScanSettings,
    cx: &mut App,
  ) -> Task<Result<Vec<PathBuf>>> {
    let scanner = match self.scanner(name, cx) {
      Ok(scanner) => scanner,
      Err(e) => return Task::ready(Err(e)),
    };
    if scanner.sane.is_some() && self.probing.load(Ordering::Relaxed) {
      return Task::ready(Err(anyhow!("scanner search running")));
    }
    let client = self.client(&scanner, cx);
    let progress = self.scan.clone();
    // claimed right away, so a second call in the same tick is refused
    if progress.read(cx).is_some() {
      return Task::ready(Err(anyhow!("a scan is already running")));
    }
    progress.write(
      cx,
      Some(ScanProgress {
        scanner: scanner.name.clone(),
        pages: 0,
      }),
    );
    // made before the task, so dropping it unpolled clears the scan as well
    let guard = ClearScan {
      progress: Some(progress.clone()),
      cx: cx.to_async(),
    };
    cx.spawn(async move |cx| {
      let mut guard = guard;
      let result = async {
        // a deleted folder would fail only after the paper is scanned
        smol::fs::create_dir_all(&settings.destination)
          .await
          .with_context(|| format!("making {}", settings.destination.display()))?;
        let stamp = jiff::Zoned::now().strftime("%Y%m%d-%H%M%S").to_string();
        let (pages, rx) = flume::unbounded();
        let on_page = move |n| {
          pages.send(n).ok();
        };
        let scan = within(SCAN_TIMEOUT, async {
          match &scanner.sane {
            Some(device) => sane::scan(device, &settings, &stamp, on_page).await,
            None => {
              let caps = escl::capabilities(&*client, &scanner).await?;
              escl::scan(&client, &scanner, &caps, &settings, &stamp, on_page).await
            }
          }
        });
        let watch = async {
          while let Ok(n) = rx.recv_async().await {
            progress.write(
              cx,
              Some(ScanProgress {
                scanner: scanner.name.clone(),
                pages: n,
              }),
            );
          }
        };
        let (files, ()) = futures_lite::future::zip(scan, watch).await;
        files
      }
      .await;
      guard.progress = None;
      progress.write(cx, None);
      result
    })
  }

  /// A state without CUPS or Avahi behind it, for UI tests.
  #[cfg(any(test, feature = "test-support"))]
  pub fn fake(
    cx: &mut App,
    printers: Vec<Printer>,
    jobs: Vec<Job>,
    scanners: Vec<Scanner>,
    network_printers: Vec<NetworkPrinter>,
  ) {
    let state = Printing {
      printers: cx.new(|_| printers),
      jobs: cx.new(|_| jobs),
      scanners: cx.new(|_| scanners),
      network_printers: cx.new(|_| network_printers),
      scan: cx.new(|_| None),
      sock: Arc::from(Path::new("/nonexistent/cups.sock")),
      system: None,
      pins: Arc::from(Path::new("/nonexistent/scanner-certs")),
      poke: flume::unbounded().0,
      sane: false,
      probing: Arc::default(),
    };
    cx.set_global(state);
  }
}

/// Ends the running scan when its task is dropped before it finished, which
/// would leave every later scan refused.
struct ClearScan {
  progress: Option<Entity<Option<ScanProgress>>>,
  cx: AsyncApp,
}

impl Drop for ClearScan {
  fn drop(&mut self) {
    let Some(progress) = self.progress.take() else {
      return;
    };
    let mut cx = self.cx.clone();
    // the app may be borrowed right now, so not in place
    self
      .cx
      .foreground_executor()
      .spawn(async move { progress.write(&mut cx, None) })
      .detach();
  }
}

const MAX_PRINT_FILE: u64 = 256 << 20;

/// A hung scanner or `scanimage` must not hold the scan state forever.
const SCAN_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const PROBE_TIMEOUT: Duration = Duration::from_secs(60);

/// `work`, or an error once `limit` has passed. Dropping `work` cancels it.
async fn within<T>(limit: Duration, work: impl Future<Output = Result<T>>) -> Result<T> {
  futures_lite::future::or(work, async {
    smol::Timer::after(limit).await;
    bail!("timed out after {}s", limit.as_secs())
  })
  .await
}

/// cups-pk-helper could not run an action: no rights, no agent, no answer.
/// Trying again would only show the password prompt again.
#[derive(Debug)]
struct AdminRefused(String);

impl std::fmt::Display for AdminRefused {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(&self.0)
  }
}

impl std::error::Error for AdminRefused {}

/// Swaps the eSCL or the SANE scanners and keeps the others, eSCL first.
fn set_scanners(
  entity: &Entity<Vec<Scanner>>,
  cx: &mut impl AppContext,
  sane: bool,
  mut next: Vec<Scanner>,
) {
  entity.update(cx, |list, cx| {
    next.extend(list.iter().filter(|s| s.sane.is_some() != sane).cloned());
    next.sort_by_key(|s| s.sane.is_some());
    // names key the selection, a SANE one equal to an eSCL one gets its device
    let mut seen = std::collections::HashSet::new();
    for s in &mut next {
      if !seen.insert(s.name.clone())
        && let Some(device) = &s.sane
      {
        s.name = format!("{} ({device})", s.name);
        seen.insert(s.name.clone());
      }
    }
    if *list != next {
      *list = next;
      cx.notify();
    }
  });
}

/// `$XDG_PICTURES_DIR`, else the home directory, where scans go by default.
pub fn pictures_dir() -> PathBuf {
  dirs::picture_dir()
    .or_else(dirs::home_dir)
    .unwrap_or_else(|| PathBuf::from("."))
}

fn unauthorized(e: &anyhow::Error) -> bool {
  e.downcast_ref::<IppError>() == Some(&IppError::Unauthorized)
}

/// Runs an admin action through cups-pk-helper, which asks polkit, which
/// asks the shell's polkit agent for a password.
async fn pk_helper<B>(conn: Option<&Connection>, method: &str, body: &B) -> Result<()>
where
  B: serde::Serialize + zbus::zvariant::DynamicType,
{
  const MECHANISM: &str = "org.opensuse.CupsPkHelper.Mechanism";
  // long enough to type a password, the shell's connection gives up after 5 s
  const PROMPT: Duration = Duration::from_secs(120);
  let call = async {
    let conn = conn.context("no system bus")?;
    let proxy = zbus::Proxy::new(conn, MECHANISM, "/", MECHANISM).await?;
    let flags = zbus::proxy::MethodFlags::AllowInteractiveAuth.into();
    let reply: Option<String> = proxy
      .call_with_flags(method, flags, body)
      .await
      .context("CUPS needs an administrator and cups-pk-helper is unavailable")?;
    anyhow::Ok(reply.unwrap_or_default())
  };
  let error = futures_lite::future::or(call, async {
    smol::Timer::after(PROMPT).await;
    bail!("no answer to the password prompt, is a polkit agent running in this session?")
  })
  .await
  .map_err(|e| AdminRefused(format!("{e:#}")))?;
  if !error.is_empty() {
    bail!("{error}");
  }
  Ok(())
}

/// Connects to CUPS and Avahi, registers the printers' UI and script module
pub async fn init(cx: &mut App, buses: &Buses) -> Result<()> {
  init_at(cx, &buses.system, Path::new(cups::SOCKET)).await?;
  ui::register(cx);
  settings::register(cx);
  register_script(cx.script_mut());
  Ok(())
}

/// Registers this integration's script modules, for the shell and the CLI
pub fn register_script(registry: &mut corona_js::ScriptRegistry) {
  registry.register::<script::PrintingModule>();
}

async fn init_at(cx: &mut App, conn: &Connection, sock: &Path) -> Result<()> {
  let (poke, pokes) = flume::unbounded();
  let sock: Arc<Path> = Arc::from(sock);
  let state = Printing {
    printers: cx.new(|_| Vec::new()),
    jobs: cx.new(|_| Vec::new()),
    scanners: cx.new(|_| Vec::new()),
    network_printers: cx.new(|_| Vec::new()),
    scan: cx.new(|_| None),
    sock: sock.clone(),
    system: Some(conn.clone()),
    pins: Arc::from(tls::pins_file()),
    poke: poke.clone(),
    sane: !cfg!(test),
    probing: Arc::default(),
  };

  // subscribe before the first snapshot so no change can slip in between
  let changes = listener::subscribe(conn).await?;
  listener::listener(cx, sock.clone(), changes, pokes, state.clone());
  listener::subscription(cx, sock, poke);
  match listener::subscribe_avahi(conn).await {
    Ok(messages) => listener::avahi_listener(
      cx,
      conn.clone(),
      messages,
      state.scanners.clone(),
      state.network_printers.clone(),
    ),
    Err(e) => tracing::warn!("no scanner discovery: {e:?}"),
  }
  cx.set_global(state);

  Ok(())
}

#[cfg(test)]
mod tests {
  use std::sync::{Arc, Mutex};

  use corona_utils::test_bus::{TestBus, settle, wait_until};
  use futures_lite::future::block_on;
  use gpui_kit::{self as gpui, TestAppContext};
  use zbus::{interface, zvariant::OwnedObjectPath};

  use super::*;
  use crate::{
    ipp::op,
    mock::{MockCups, job, printer},
  };

  fn start(cx: &mut TestAppContext, cups: &MockCups) -> (TestBus, Connection) {
    cx.executor().allow_parking();
    let (bus, conn) = TestBus::with_conn();
    let sock = cups.sock.clone();
    cx.update(|cx| {
      cx.foreground_executor()
        .clone()
        .block_on(init_at(cx, &conn, &sock))
        .unwrap()
    });
    (bus, conn)
  }

  fn names(cx: &mut TestAppContext) -> Vec<(String, PrinterState)> {
    cx.update(|cx| {
      cx.printing()
        .list_printers(cx)
        .iter()
        .map(|p| (p.name.clone(), p.state))
        .collect()
    })
  }

  fn job_states(cx: &mut TestAppContext) -> Vec<(i32, JobState)> {
    cx.update(|cx| {
      cx.printing()
        .list_jobs(cx)
        .iter()
        .map(|j| (j.id, j.state))
        .collect()
    })
  }

  fn run<T: 'static>(cx: &mut TestAppContext, f: impl Future<Output = T> + 'static) -> T {
    let done = Arc::new(Mutex::new(None));
    let slot = done.clone();
    cx.update(|cx| {
      cx.spawn(async move |_| *slot.lock().unwrap() = Some(f.await))
        .detach()
    });
    wait_until(cx, |_| done.lock().unwrap().is_some());
    done.lock().unwrap().take().unwrap()
  }

  #[gpui::test]
  fn reads_printers_and_jobs_and_subscribes(cx: &mut TestAppContext) {
    let cups = MockCups::start();
    {
      let mut state = cups.state.lock().unwrap();
      state.printers = vec![printer("Office", 3), printer("Lab", 4)];
      state.jobs = vec![job(1, "Office", 9), job(2, "Lab", 5)];
    }
    let (_bus, _conn) = start(cx, &cups);
    wait_until(cx, |cx| names(cx).len() == 2);
    assert_eq!(
      names(cx),
      [
        ("Lab".into(), PrinterState::Processing),
        ("Office".into(), PrinterState::Idle)
      ]
    );
    wait_until(cx, |cx| job_states(cx).len() == 2);
    assert_eq!(
      job_states(cx),
      [(2, JobState::Processing), (1, JobState::Completed)]
    );
    wait_until(cx, |_| {
      cups.ops().contains(&op::CREATE_PRINTER_SUBSCRIPTION)
    });
    let state = cups.state.lock().unwrap();
    let sub = state
      .requests
      .iter()
      .find(|r| r.code == op::CREATE_PRINTER_SUBSCRIPTION)
      .unwrap();
    let group = sub.groups(ipp::tag::SUBSCRIPTION).next().unwrap();
    assert_eq!(
      group.get("notify-recipient-uri").unwrap().str(),
      Some("dbus://")
    );
    assert!(
      group
        .get("notify-events")
        .unwrap()
        .strs()
        .any(|e| e == "job-created")
    );
    drop(state);
    cx.update(|cx| assert!(cx.printing().busy(cx)));
  }

  #[gpui::test]
  fn follows_notifier_signals(cx: &mut TestAppContext) {
    let cups = MockCups::start();
    cups.state.lock().unwrap().printers = vec![printer("Office", 3)];
    let (bus, _conn) = start(cx, &cups);
    wait_until(cx, |cx| names(cx).len() == 1);
    // a new subscription re-reads once, let that pass first
    wait_until(cx, |_| {
      cups.ops().contains(&op::CREATE_PRINTER_SUBSCRIPTION)
    });
    settle(cx);

    cups.state.lock().unwrap().printers[0] = printer("Office", 5);
    settle(cx);
    assert_eq!(names(cx)[0].1, PrinterState::Idle, "no polling");

    let notifier = block_on(bus.conn());
    block_on(notifier.emit_signal(
      None::<()>,
      "/org/cups/cupsd/Notifier",
      "org.cups.cupsd.Notifier",
      "PrinterStateChanged",
      &(
        "printer stopped",
        "ipp://localhost/printers/Office",
        "Office",
        5u32,
        "paused",
        false,
      ),
    ))
    .unwrap();
    wait_until(cx, |cx| names(cx)[0].1 == PrinterState::Stopped);

    cups.state.lock().unwrap().jobs = vec![job(3, "Office", 3)];
    block_on(notifier.emit_signal(
      None::<()>,
      "/org/cups/cupsd/Notifier",
      "org.cups.cupsd.Notifier",
      "JobCreated",
      &(
        "job created",
        "ipp://localhost/printers/Office",
        "Office",
        3u32,
        "none",
        true,
        3u32,
        3u32,
        "",
        "job 3",
        0u32,
      ),
    ))
    .unwrap();
    wait_until(cx, |cx| job_states(cx) == [(3, JobState::Pending)]);
  }

  #[gpui::test]
  fn cancels_prints_and_pauses(cx: &mut TestAppContext) {
    let cups = MockCups::start();
    {
      let mut state = cups.state.lock().unwrap();
      state.printers = vec![printer("Office", 3)];
      state.jobs = vec![job(1, "Office", 5)];
    }
    let (_bus, _conn) = start(cx, &cups);
    wait_until(cx, |cx| job_states(cx).len() == 1);

    let cancel = cx.update(|cx| cx.printing().cancel_job(1));
    run(cx, cancel).unwrap();
    // the action refreshes without a notifier signal
    wait_until(cx, |cx| job_states(cx) == [(1, JobState::Canceled)]);

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("hello.txt");
    std::fs::write(&file, "hello").unwrap();
    let print = cx.update(|cx| cx.printing().print_file("Office", file));
    run(cx, print).unwrap();
    {
      let state = cups.state.lock().unwrap();
      let request = state
        .requests
        .iter()
        .find(|r| r.code == op::PRINT_JOB)
        .unwrap();
      assert_eq!(request.data, b"hello");
      assert_eq!(
        request.operation("job-name").unwrap().str(),
        Some("hello.txt")
      );
      assert_eq!(
        request.operation("printer-uri").unwrap().str(),
        Some("ipp://localhost/printers/Office")
      );
    }
    wait_until(cx, |cx| job_states(cx).iter().any(|j| j.0 == 101));

    let pause = cx.update(|cx| cx.printing().set_paused("Office", true));
    run(cx, pause).unwrap();
    wait_until(cx, |cx| names(cx)[0].1 == PrinterState::Stopped);
  }

  #[test]
  fn no_printers_is_an_empty_list() {
    let cups = MockCups::start();
    assert_eq!(block_on(cups::printers(&cups.sock)).unwrap(), []);
    // other refusals still fail
    cups.state.lock().unwrap().broken = true;
    assert!(block_on(cups::printers(&cups.sock)).is_err());
  }

  #[test]
  fn http_401_does_not_hang_and_peercred_is_tried() {
    let cups = MockCups::start();
    cups.state.lock().unwrap().http_auth = true;
    let started = std::time::Instant::now();
    let err = block_on(cups::set_paused(&cups.sock, "Office", true)).unwrap_err();
    assert!(unauthorized(&err), "{err}");
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    assert_eq!(cups.state.lock().unwrap().peercred_tries, 1);

    // a user CUPS counts as admin gets through without polkit
    cups.state.lock().unwrap().peercred_admin = true;
    block_on(cups::set_paused(&cups.sock, "Office", true)).unwrap();
  }

  /// A printer that announces IPP but cannot do IPP Everywhere.
  fn add_old_printer(cx: &mut TestAppContext, ppds: &[(&str, &str)]) -> (MockCups, Result<()>) {
    let cups = MockCups::start();
    {
      let mut state = cups.state.lock().unwrap();
      state.not_driverless = true;
      state.ppds = ppds
        .iter()
        .map(|(n, m)| (n.to_string(), m.to_string()))
        .collect();
    }
    let (_bus, _conn) = start(cx, &cups);
    let printer = NetworkPrinter {
      name: "Samsung CLX-3180 Series (SEC00)".into(),
      model: Some("Samsung CLX-3180 Series".into()),
      address: "10.0.0.5".into(),
      uri: "dnssd://Samsung._ipp._tcp.local/".into(),
      direct_uri: "ipp://10.0.0.5:631/ipp/print".into(),
    };
    cx.update(|cx| {
      let printing = cx.printing().clone();
      printing
        .network_printers
        .update(cx, |p, _| *p = vec![printer]);
    });
    let add = cx.update(|cx| {
      cx.printing()
        .add_printer("Samsung CLX-3180 Series (SEC00)", cx)
    });
    let result = run(cx, add);
    (cups, result)
  }

  fn ppd_names(cups: &MockCups) -> Vec<String> {
    let state = cups.state.lock().unwrap();
    state
      .requests
      .iter()
      .filter(|r| r.code == op::CUPS_ADD_MODIFY_PRINTER)
      .filter_map(|r| r.operation("ppd-name")?.str().map(String::from))
      .collect()
  }

  #[gpui::test]
  fn old_printers_get_an_installed_driver(cx: &mut TestAppContext) {
    let ppds = [
      (
        "drv:///sample.drv/generic.ppd",
        "Generic PostScript Printer",
      ),
      ("splix/clx3180.ppd", "Samsung CLX-3180, SpliX V2.0.1"),
      // same model number, another make
      ("acme/clx3180.ppd", "Acme CLX-3180"),
      (
        "splix/clx3180fn.ppd",
        "Samsung CLX-3180FN Extra Long Name, SpliX V2.0.1",
      ),
    ];
    let (cups, result) = add_old_printer(cx, &ppds);
    result.unwrap();
    assert_eq!(
      ppd_names(&cups),
      ["everywhere", "everywhere", "splix/clx3180.ppd"]
    );
    let state = cups.state.lock().unwrap();
    let last = state
      .requests
      .iter()
      .rfind(|r| r.code == op::CUPS_ADD_MODIFY_PRINTER)
      .unwrap();
    let device = last
      .groups(ipp::tag::PRINTER)
      .find_map(|g| g.get("device-uri")?.str());
    assert_eq!(device, Some("ipp://10.0.0.5:631/ipp/print?version=1.0"));
  }

  #[gpui::test]
  fn old_printers_without_a_driver_say_so(cx: &mut TestAppContext) {
    let (_cups, result) = add_old_printer(cx, &[("drv:///sample.drv/generic.ppd", "Generic")]);
    let error = result.unwrap_err().to_string();
    assert!(error.contains("without a driver"), "{error}");
    assert!(error.contains("invalid data"), "{error}");
  }

  #[test]
  fn admin_refusal_is_unauthorized() {
    let cups = MockCups::start();
    cups.state.lock().unwrap().deny_admin = true;
    let err = block_on(cups::set_paused(&cups.sock, "Office", true)).unwrap_err();
    assert!(unauthorized(&err), "{err}");
    let ok = block_on(cups::cancel_job(&cups.sock, 1));
    assert!(ok.is_ok());
  }

  /// cups-pk-helper answers every method with a plain string, empty on success.
  struct FakePkHelper(&'static str);

  #[interface(name = "org.opensuse.CupsPkHelper.Mechanism")]
  impl FakePkHelper {
    fn printer_add(
      &self,
      _name: String,
      _uri: String,
      _ppd: String,
      _info: String,
      _location: String,
    ) -> String {
      self.0.into()
    }
  }

  #[test]
  fn pk_helper_reads_a_plain_string_reply() {
    let (bus, helper) = TestBus::with_conn();
    let client = block_on(bus.conn());
    block_on(async {
      helper
        .object_server()
        .at("/", FakePkHelper(""))
        .await
        .unwrap();
      helper
        .request_name("org.opensuse.CupsPkHelper.Mechanism")
        .await
        .unwrap();
    });
    let add = ("Office", "ipp://x", "", "Office", "");
    block_on(pk_helper(Some(&client), "PrinterAdd", &add)).unwrap();
  }

  #[gpui::test]
  fn broken_cupsd_keeps_printers(cx: &mut TestAppContext) {
    let cups = MockCups::start();
    cups.state.lock().unwrap().printers = vec![printer("Office", 3)];
    let (_bus, _conn) = start(cx, &cups);
    wait_until(cx, |cx| names(cx).len() == 1);
    cups.state.lock().unwrap().broken = true;
    cx.update(|cx| cx.printing().refresh());
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    assert_eq!(names(cx).len(), 1);
  }

  /// A fake avahi-daemon that resolves every service to `address`.
  struct FakeAvahi {
    browsers: Arc<Mutex<Vec<String>>>,
    address: Arc<Mutex<String>>,
  }

  #[interface(name = "org.freedesktop.Avahi.Server")]
  impl FakeAvahi {
    fn service_browser_new(
      &self,
      _interface: i32,
      _protocol: i32,
      kind: String,
      _domain: String,
      _flags: u32,
    ) -> OwnedObjectPath {
      let mut browsers = self.browsers.lock().unwrap();
      browsers.push(kind);
      OwnedObjectPath::try_from(format!("/Client1/ServiceBrowser{}", browsers.len())).unwrap()
    }

    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    fn resolve_service(
      &self,
      interface: i32,
      protocol: i32,
      name: String,
      kind: String,
      domain: String,
      aprotocol: i32,
      _flags: u32,
    ) -> (
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
    ) {
      let txt = vec![b"rs=eSCL".to_vec(), format!("ty={name} model").into_bytes()];
      (
        interface,
        protocol,
        name,
        kind,
        domain,
        "host.local".into(),
        aprotocol,
        self.address.lock().unwrap().clone(),
        8080,
        txt,
        0,
      )
    }
  }

  fn emit(conn: &Connection, member: &str, name: &str, kind: &str) {
    block_on(conn.emit_signal(
      None::<()>,
      "/Client1/ServiceBrowser1",
      "org.freedesktop.Avahi.ServiceBrowser",
      member,
      &(2i32, 0i32, name, kind, "local", 0u32),
    ))
    .unwrap();
  }

  fn scanner_names(cx: &mut TestAppContext) -> Vec<(String, bool)> {
    cx.update(|cx| {
      cx.printing()
        .list_scanners(cx)
        .iter()
        .map(|s| (s.name.clone(), s.tls))
        .collect()
    })
  }

  #[gpui::test]
  fn follows_avahi_announcements(cx: &mut TestAppContext) {
    let cups = MockCups::start();
    cx.executor().allow_parking();
    let (bus, avahi) = TestBus::with_conn();
    let browsers = Arc::new(Mutex::new(Vec::new()));
    let address = Arc::new(Mutex::new("10.0.0.9".to_string()));
    block_on(async {
      avahi
        .object_server()
        .at(
          "/",
          FakeAvahi {
            browsers: browsers.clone(),
            address: address.clone(),
          },
        )
        .await
        .unwrap();
      avahi.request_name("org.freedesktop.Avahi").await.unwrap();
    });
    let conn = block_on(bus.conn());
    let sock = cups.sock.clone();
    cx.update(|cx| {
      cx.foreground_executor()
        .clone()
        .block_on(init_at(cx, &conn, &sock))
        .unwrap()
    });
    wait_until(cx, |_| browsers.lock().unwrap().len() == 4);
    assert_eq!(
      *browsers.lock().unwrap(),
      [avahi::SCAN, avahi::SCANS, avahi::IPP, avahi::IPPS]
    );

    emit(&avahi, "ItemNew", "Secure", avahi::SCANS);
    emit(&avahi, "ItemNew", "Brother", avahi::SCANS);
    emit(&avahi, "ItemNew", "Brother", avahi::SCAN);
    // the TLS announcement wins over the plain one of the same scanner
    wait_until(cx, |cx| {
      scanner_names(cx) == [("Brother".into(), true), ("Secure".into(), true)]
    });
    let base = cx.update(|cx| cx.printing().list_scanners(cx)[0].base_url());
    assert_eq!(base, "https://10.0.0.9:8080/eSCL");

    emit(&avahi, "ItemRemove", "Secure", avahi::SCANS);
    wait_until(cx, |cx| scanner_names(cx) == [("Brother".into(), true)]);
    // without its TLS announcement the plain one is used
    emit(&avahi, "ItemRemove", "Brother", avahi::SCANS);
    wait_until(cx, |cx| scanner_names(cx) == [("Brother".into(), false)]);

    // a network printer without a queue can be added, by address when
    // cupsd cannot resolve its `.local` name
    cups.state.lock().unwrap().unresolvable_dnssd = true;
    emit(&avahi, "ItemNew", "Laser Jet", avahi::IPP);
    emit(&avahi, "ItemNew", "Laser Jet", avahi::IPPS);
    let new = |cx: &mut TestAppContext| {
      cx.update(|cx| {
        let printing = cx.printing();
        let found = printing.new_printers(cx);
        found.iter().map(|p| p.uri.clone()).collect::<Vec<_>>()
      })
    };
    wait_until(cx, |cx| {
      new(cx) == ["dnssd://Laser%20Jet._ipps._tcp.local/"]
    });
    // the printer got another address since it was announced
    *address.lock().unwrap() = "10.0.0.10".into();
    let add = cx.update(|cx| cx.printing().add_printer("Laser Jet", cx));
    run(cx, add).unwrap();
    {
      let state = cups.state.lock().unwrap();
      let request = state
        .requests
        .iter()
        .find(|r| r.code == op::CUPS_ADD_MODIFY_PRINTER)
        .unwrap();
      assert_eq!(
        request.operation("ppd-name").unwrap().str(),
        Some("everywhere")
      );
      let tries: Vec<_> = state
        .requests
        .iter()
        .filter(|r| r.code == op::CUPS_ADD_MODIFY_PRINTER)
        .filter_map(|r| {
          let group = r.groups(ipp::tag::PRINTER).next()?;
          group.get("device-uri")?.str().map(String::from)
        })
        .collect();
      assert_eq!(
        tries,
        [
          "dnssd://Laser%20Jet._ipps._tcp.local/",
          "ipps://10.0.0.10:8080/ipp/print"
        ]
      );
      assert_eq!(
        request.operation("printer-uri").unwrap().str(),
        Some("ipp://localhost/printers/Laser_Jet")
      );
    }
    // the new queue shows, and the printer is no longer offered
    wait_until(cx, |cx| names(cx).iter().any(|(n, _)| n == "Laser_Jet"));
    assert!(new(cx).is_empty());

    // avahi-daemon quitting forgets every scanner and printer
    drop(avahi);
    wait_until(cx, |cx| scanner_names(cx).is_empty());
    cx.update(|cx| assert!(cx.printing().network_printers.read(cx).is_empty()));
  }
}
