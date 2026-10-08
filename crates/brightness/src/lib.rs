use std::{
  collections::HashSet,
  sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
  },
  thread,
  time::Duration,
};

use anyhow::{Context, Result, bail};
use corona_config::{BrightnessConfig, ConfigProvider, observe_section};
use gpui_kit::{App, AppContext, Entity, Global};
use zbus::Connection;

use crate::{
  ddc::{DdcCommand, Update},
  outputs::Output,
};

pub use crate::state::{Display, DisplayKind, Unavailable};

mod backlight;
mod ddc;
mod outputs;
mod state;

#[derive(Clone)]
pub struct Brightness {
  pub displays: Entity<Vec<Display>>,
  pub ddcutil_available: bool,
  pub ddcutil_enabled: bool,
  conn: Connection,
  ddc: Option<flume::Sender<DdcCommand>>,
}

impl Global for Brightness {}

pub trait BrightnessExt {
  fn brightness(&self) -> &Brightness;
}

impl BrightnessExt for App {
  fn brightness(&self) -> &Brightness {
    self.global::<Brightness>()
  }
}

impl Brightness {
  pub fn list_displays<'c>(&self, cx: &'c App) -> &'c [Display] {
    self.displays.read(cx)
  }

  pub fn display<'c>(&self, id: &str, cx: &'c App) -> Option<&'c Display> {
    self.list_displays(cx).iter().find(|d| d.id == id)
  }

  pub fn set_brightness(
    &self,
    id: &str,
    brightness: u32,
    cx: &App,
  ) -> impl Future<Output = Result<()>> + use<> {
    let conn = self.conn.clone();
    let ddc = self.ddc.clone();
    let display = self
      .display(id, cx)
      .with_context(|| format!("no display {id}"))
      .and_then(|d| match d.unavailable {
        None => Ok((d.kind, d.max)),
        Some(reason) => bail!("{} cannot be changed: {reason:?}", d.id),
      });
    let id = id.to_string();
    async move {
      let (kind, max) = display?;
      let brightness = brightness.min(max);
      match (kind, id.split_once('/')) {
        (DisplayKind::Backlight, Some((_, name))) => backlight::set(&conn, name, brightness).await,
        (DisplayKind::External, Some((_, bus))) => {
          let ddc = ddc.context("ddcutil is off")?;
          ddc.send(DdcCommand::Set {
            bus: bus.parse()?,
            brightness,
          })?;
          Ok(())
        }
        _ => bail!("malformed display id {id}"),
      }
    }
  }
}

enum Event {
  Sysfs {
    backlights: Vec<Display>,
    outputs: Vec<Output>,
  },
  Ddc(Update),
}

#[derive(Clone, Debug, PartialEq)]
enum Ddc {
  Off(Unavailable),
  Detecting,
  Detected(Vec<Display>),
  DetectFailed,
}

fn merge(
  backlights: &[Display],
  outputs: &[Output],
  ddc: &Ddc,
  failed: &HashSet<String>,
) -> Vec<Display> {
  let detected: &[Display] = match ddc {
    Ddc::Detected(displays) => displays,
    _ => &[],
  };
  let mark_failed = |mut display: Display| {
    if failed.contains(&display.id) {
      display.unavailable = Some(Unavailable::Failed);
    }
    display
  };
  let builtin = |output: &Output| {
    backlights
      .iter()
      .any(|b| b.output.as_ref() == Some(&output.name))
  };

  let monitors: Vec<Display> = outputs
    .iter()
    .filter(|o| !builtin(o))
    .map(|output| {
      match detected
        .iter()
        .find(|d| d.output.as_ref() == Some(&output.name))
      {
        Some(display) => mark_failed(display.clone()),
        None => Display {
          id: format!("output/{}", output.name),
          output: Some(output.name.clone()),
          name: Some(output.model.clone().unwrap_or_else(|| output.name.clone())),
          kind: DisplayKind::External,
          brightness: 0,
          max: 0,
          unavailable: Some(match ddc {
            Ddc::Off(reason) => *reason,
            Ddc::Detecting => Unavailable::Detecting,
            Ddc::Detected(_) => Unavailable::Unsupported,
            Ddc::DetectFailed => Unavailable::Failed,
          }),
        },
      }
    })
    .collect();
  // monitors `ddcutil` could not tie to an output, or that share one, still work
  let unmatched = detected
    .iter()
    .filter(|d| !monitors.iter().any(|m| m.id == d.id))
    .cloned()
    .map(mark_failed)
    .collect::<Vec<_>>();

  backlights
    .iter()
    .cloned()
    .chain(monitors)
    .chain(unmatched)
    .collect()
}

pub fn init(cx: &mut App, conn: &Connection) -> Result<()> {
  let ddcutil_available = ddc::available();
  let ddcutil_enabled = cx.config().brightness.enable_ddcutil;
  let (events_tx, events) = flume::unbounded();

  // seconds, kept current by the settings below
  let poll = Arc::new(AtomicU64::new(cx.config().brightness.poll_seconds));
  observe_section(cx, |c| &c.brightness, {
    let poll = poll.clone();
    move |config: &BrightnessConfig, _| poll.store(config.poll_seconds, Ordering::Relaxed)
  });
  let sysfs = events_tx.clone();
  thread::spawn(move || {
    let mut last = None;
    loop {
      let drm = backlight::sys(backlight::DRM);
      let current = (
        backlight::list(&backlight::sys(backlight::BACKLIGHT), &drm),
        outputs::connected(&drm),
      );
      if last.as_ref() != Some(&current) {
        last = Some(current.clone());
        let (backlights, outputs) = current;
        if sysfs
          .send(Event::Sysfs {
            backlights,
            outputs,
          })
          .is_err()
        {
          break;
        }
      }
      thread::sleep(Duration::from_secs(poll.load(Ordering::Relaxed).max(1)));
    }
  });

  let ddc = (ddcutil_enabled && ddcutil_available).then(|| {
    let (commands_tx, commands) = flume::unbounded();
    let (updates_tx, updates) = flume::unbounded();
    ddc::worker(commands, updates_tx);
    let events = events_tx.clone();
    thread::spawn(move || {
      for update in updates.iter() {
        if events.send(Event::Ddc(update)).is_err() {
          break;
        }
      }
    });
    commands_tx
  });
  if ddcutil_enabled && !ddcutil_available {
    tracing::warn!("enable_ddcutil is set, but ddcutil is not on the PATH");
  }

  let state = Brightness {
    displays: cx.new(|_| Vec::new()),
    ddcutil_available,
    ddcutil_enabled,
    conn: conn.clone(),
    ddc,
  };

  let displays = state.displays.clone();
  let mut ddc = match (ddcutil_enabled, ddcutil_available) {
    (false, _) => Ddc::Off(Unavailable::DdcutilDisabled),
    (true, false) => Ddc::Off(Unavailable::DdcutilMissing),
    (true, true) => Ddc::Detecting,
  };
  cx.spawn(async move |cx| {
    let (mut backlights, mut outputs) = (Vec::new(), Vec::new());
    let mut failed = HashSet::new();
    while let Ok(event) = events.recv_async().await {
      match event {
        Event::Sysfs {
          backlights: b,
          outputs: o,
        } => (backlights, outputs) = (b, o),
        Event::Ddc(Update::Displays(list)) => ddc = Ddc::Detected(list),
        Event::Ddc(Update::DetectFailed) => ddc = Ddc::DetectFailed,
        Event::Ddc(Update::Brightness { bus, brightness }) => {
          let id = format!("ddc/{bus}");
          if let Ddc::Detected(list) = &mut ddc
            && let Some(display) = list.iter_mut().find(|d| d.id == id)
          {
            display.brightness = brightness;
          }
        }
        Event::Ddc(Update::Failed { bus }) => _ = failed.insert(format!("ddc/{bus}")),
        Event::Ddc(Update::Recovered { bus }) => _ = failed.remove(&format!("ddc/{bus}")),
      }
      displays.write(cx, merge(&backlights, &outputs, &ddc, &failed));
    }
  })
  .detach();

  cx.set_global(state);
  Ok(())
}

/// A scripted `ddcutil` on `PATH`; nextest runs each test in its own process, so changing the
/// environment is safe there.
#[cfg(test)]
pub(crate) mod fake_ddcutil {
  use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};

  pub(crate) const DETECT: &str = "\
Display 1
   I2C bus:  /dev/i2c-5
   DRM_connector:           card1-DP-1
   EDID synopsis:
      Model:                DELL U2720Q

Display 2
   I2C bus:  /dev/i2c-6
   DRM_connector:           card1-DP-2
";

  const SCRIPT: &str = r#"#!/bin/sh
dir="$(dirname "$0")"
echo "$*" >> "$dir/calls"
case "$2" in
  detect)
    [ -f "$dir/detect-fails" ] && exit 1
    cat "$dir/detect"
    exit 0 ;;
esac
bus="$3"
case "$4" in
  getvcp)
    [ -f "$dir/getvcp-$bus" ] || exit 1
    cat "$dir/getvcp-$bus" ;;
  setvcp)
    [ -f "$dir/hang" ] && exec sleep 5
    [ -f "$dir/setvcp-fails-$bus" ] && exit 1
    exit 0 ;;
esac
"#;

  pub(crate) struct FakeDdcutil {
    pub dir: tempfile::TempDir,
  }

  impl FakeDdcutil {
    /// bus 5 reads 50 of 100, bus 6 has no DDC/CI
    pub(crate) fn install() -> Self {
      let dir = tempfile::tempdir().unwrap();
      let script = dir.path().join("ddcutil");
      fs::write(&script, SCRIPT).unwrap();
      fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
      fs::write(dir.path().join("detect"), DETECT).unwrap();
      fs::write(dir.path().join("getvcp-5"), "VCP 10 C 50 100\n").unwrap();
      let path = std::env::var_os("PATH").unwrap_or_default();
      let mut paths = vec![dir.path().to_path_buf()];
      paths.extend(std::env::split_paths(&path));
      unsafe { std::env::set_var("PATH", std::env::join_paths(paths).unwrap()) };
      Self { dir }
    }

    pub(crate) fn file(&self, name: &str) -> PathBuf {
      self.dir.path().join(name)
    }

    pub(crate) fn touch(&self, name: &str) {
      fs::write(self.file(name), "").unwrap();
    }

    pub(crate) fn calls(&self) -> Vec<String> {
      fs::read_to_string(self.file("calls"))
        .unwrap_or_default()
        .lines()
        .map(Into::into)
        .collect()
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn output(name: &str, model: Option<&str>) -> Output {
    Output {
      name: name.into(),
      model: model.map(Into::into),
    }
  }

  fn display(id: &str, output: &str, kind: DisplayKind) -> Display {
    Display {
      id: id.into(),
      output: Some(output.into()),
      name: Some(id.into()),
      kind,
      brightness: 40,
      max: 100,
      unavailable: None,
    }
  }

  #[test]
  fn merge() {
    let panel = display("backlight/intel_backlight", "eDP-1", DisplayKind::Backlight);
    let outputs = [
      output("eDP-1", None),
      output("DP-1", Some("DELL U2720Q")),
      output("HDMI-A-1", Some("LG TV")),
    ];
    let backlights = [panel.clone()];
    let unavailable = |ddc: &Ddc, failed: &HashSet<String>| {
      super::merge(&backlights, &outputs, ddc, failed)
        .into_iter()
        .map(|d| (d.name, d.unavailable))
        .collect::<Vec<_>>()
    };
    let none = HashSet::new();

    // the panel is listed once, monitors get the reason they cannot be changed
    assert_eq!(
      unavailable(&Ddc::Off(Unavailable::DdcutilMissing), &none),
      [
        (Some("backlight/intel_backlight".into()), None),
        (
          Some("DELL U2720Q".into()),
          Some(Unavailable::DdcutilMissing)
        ),
        (Some("LG TV".into()), Some(Unavailable::DdcutilMissing)),
      ]
    );

    // the detected monitor is controllable, the other has no DDC/CI
    let detected = Ddc::Detected(vec![display("ddc/5", "DP-1", DisplayKind::External)]);
    assert_eq!(
      unavailable(&detected, &none),
      [
        (Some("backlight/intel_backlight".into()), None),
        (Some("ddc/5".into()), None),
        (Some("LG TV".into()), Some(Unavailable::Unsupported)),
      ]
    );

    let failed = HashSet::from(["ddc/5".to_string()]);
    assert_eq!(
      unavailable(&detected, &failed)[1].1,
      Some(Unavailable::Failed)
    );
  }

  #[test]
  fn merge_edges() {
    let none = HashSet::new();
    let outputs = [output("DP-1", None)];
    // nameless monitors are named by their output
    let off = super::merge(
      &[],
      &outputs,
      &Ddc::Off(Unavailable::DdcutilDisabled),
      &none,
    );
    assert_eq!(
      off,
      [Display {
        id: "output/DP-1".into(),
        output: Some("DP-1".into()),
        name: Some("DP-1".into()),
        kind: DisplayKind::External,
        brightness: 0,
        max: 0,
        unavailable: Some(Unavailable::DdcutilDisabled),
      }]
    );
    for (ddc, reason) in [
      (Ddc::Detecting, Unavailable::Detecting),
      (Ddc::DetectFailed, Unavailable::Failed),
      (Ddc::Detected(vec![]), Unavailable::Unsupported),
    ] {
      assert_eq!(
        super::merge(&[], &outputs, &ddc, &none)[0].unavailable,
        Some(reason)
      );
    }

    // monitors ddcutil could not tie to a connected output come last, failures marked
    let mut loose = display("ddc/9", "DP-9", DisplayKind::External);
    let mut unknown = display("ddc/8", "", DisplayKind::External);
    unknown.output = None;
    let detected = Ddc::Detected(vec![loose.clone(), unknown.clone()]);
    let failed = HashSet::from(["ddc/9".to_string()]);
    loose.unavailable = Some(Unavailable::Failed);
    let merged = super::merge(&[], &outputs, &detected, &failed);
    assert_eq!(merged.len(), 3);
    assert_eq!(merged[1..], [loose, unknown]);

    // a backlight without an output does not hide any monitor
    let mut panel = display("backlight/acpi_video0", "", DisplayKind::Backlight);
    panel.output = None;
    let merged = super::merge(&[panel], &outputs, &Ddc::Detecting, &none);
    assert_eq!(merged.len(), 2);
    assert!(super::merge(&[], &[], &Ddc::Detecting, &none).is_empty());
  }

  #[test]
  fn merge_keeps_every_detected_monitor() {
    let detected = Ddc::Detected(vec![
      display("ddc/5", "DP-1", DisplayKind::External),
      display("ddc/6", "DP-1", DisplayKind::External),
    ]);
    let merged = super::merge(&[], &[output("DP-1", None)], &detected, &HashSet::new());
    assert_eq!(merged.len(), 2);
  }
}

#[cfg(test)]
mod global_tests {
  use std::{fs, time::Instant};

  use corona_config::Config;
  use corona_utils::test_bus::TestBus;
  use futures_lite::future::block_on;
  use gpui_kit::{self as gpui, TestAppContext};

  use super::{fake_ddcutil::FakeDdcutil, *};

  struct Session {
    calls: std::sync::Arc<std::sync::Mutex<Vec<(String, String, u32)>>>,
    refuse: bool,
  }

  #[zbus::interface(name = "org.freedesktop.login1.Session")]
  impl Session {
    fn set_brightness(
      &self,
      subsystem: String,
      name: String,
      brightness: u32,
    ) -> zbus::fdo::Result<()> {
      if self.refuse {
        return Err(zbus::fdo::Error::AccessDenied("no".into()));
      }
      self
        .calls
        .lock()
        .unwrap()
        .push((subsystem, name, brightness));
      Ok(())
    }
  }

  /// a sysfs tree with an Intel panel on eDP-1 and a monitor on DP-1
  fn sysfs() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let write = |file: &str, value: &str| {
      let path = root.path().join(file);
      fs::create_dir_all(path.parent().unwrap()).unwrap();
      fs::write(path, value).unwrap();
    };
    write("class/backlight/intel_backlight/brightness", "300\n");
    write("class/backlight/intel_backlight/max_brightness", "1200\n");
    write("class/drm/card1-eDP-1/status", "connected\n");
    fs::create_dir_all(root.path().join("class/drm/card1-eDP-1/intel_backlight")).unwrap();
    write("class/drm/card1-DP-1/status", "connected\n");
    write("class/drm/card1-HDMI-A-1/status", "disconnected\n");
    unsafe { std::env::set_var("CORONA_TEST_SYSFS", root.path()) };
    root
  }

  fn state(cx: &mut TestAppContext, conn: &Connection, ddc: Option<flume::Sender<DdcCommand>>) {
    let panel = Display {
      id: "backlight/intel_backlight".into(),
      output: Some("eDP-1".into()),
      name: None,
      kind: DisplayKind::Backlight,
      brightness: 300,
      max: 1200,
      unavailable: None,
    };
    let monitor = Display {
      id: "ddc/5".into(),
      output: Some("DP-1".into()),
      name: None,
      kind: DisplayKind::External,
      brightness: 50,
      max: 100,
      unavailable: None,
    };
    let broken = Display {
      id: "ddc/x".into(),
      ..monitor.clone()
    };
    let off = Display {
      id: "output/HDMI-A-1".into(),
      unavailable: Some(Unavailable::Unsupported),
      ..monitor.clone()
    };
    let weird = Display {
      id: "weird".into(),
      ..monitor.clone()
    };
    cx.update(|cx| {
      let state = Brightness {
        displays: cx.new(|_| vec![panel, monitor, broken, off, weird]),
        ddcutil_available: true,
        ddcutil_enabled: true,
        conn: conn.clone(),
        ddc,
      };
      cx.set_global(state);
    });
  }

  fn set(cx: &mut TestAppContext, id: &str, value: u32) -> Result<()> {
    let task = cx.update(|cx| cx.brightness().set_brightness(id, value, cx));
    block_on(task)
  }

  #[gpui::test]
  fn set_brightness_checks_the_display(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let conn = block_on(bus.conn());
    let (tx, rx) = flume::unbounded();
    state(cx, &conn, Some(tx));

    assert_eq!(
      set(cx, "nope", 1).unwrap_err().to_string(),
      "no display nope"
    );
    assert_eq!(
      set(cx, "output/HDMI-A-1", 1).unwrap_err().to_string(),
      "output/HDMI-A-1 cannot be changed: Unsupported"
    );
    assert_eq!(
      set(cx, "weird", 1).unwrap_err().to_string(),
      "malformed display id weird"
    );
    assert!(set(cx, "ddc/x", 1).is_err());
    assert!(rx.is_empty());

    // clamped to the display's maximum
    set(cx, "ddc/5", 500).unwrap();
    set(cx, "ddc/5", 30).unwrap();
    let sent: Vec<_> = rx
      .try_iter()
      .map(|DdcCommand::Set { bus, brightness }| (bus, brightness))
      .collect();
    assert_eq!(sent, [(5, 100), (5, 30)]);
  }

  #[gpui::test]
  fn set_brightness_without_ddcutil(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let conn = block_on(bus.conn());
    state(cx, &conn, None);
    assert_eq!(
      set(cx, "ddc/5", 1).unwrap_err().to_string(),
      "ddcutil is off"
    );
  }

  #[gpui::test]
  fn backlights_go_through_logind(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let conn = block_on(bus.conn());
    let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let logind = block_on(async {
      let logind = bus.conn().await;
      logind
        .object_server()
        .at(
          "/org/freedesktop/login1/session/auto",
          Session {
            calls: calls.clone(),
            refuse: false,
          },
        )
        .await
        .unwrap();
      logind.request_name("org.freedesktop.login1").await.unwrap();
      logind
    });
    let root = sysfs();
    state(cx, &conn, None);
    set(cx, "backlight/intel_backlight", 5000).unwrap();
    assert_eq!(
      calls.lock().unwrap().as_slice(),
      [("backlight".to_string(), "intel_backlight".to_string(), 1200)]
    );
    // sysfs untouched
    let file = root
      .path()
      .join("class/backlight/intel_backlight/brightness");
    assert_eq!(fs::read_to_string(file).unwrap(), "300\n");
    drop(logind);
  }

  #[gpui::test]
  fn refused_by_logind_writes_sysfs(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let conn = block_on(bus.conn());
    let _logind = block_on(async {
      let logind = bus.conn().await;
      logind
        .object_server()
        .at(
          "/org/freedesktop/login1/session/auto",
          Session {
            calls: Default::default(),
            refuse: true,
          },
        )
        .await
        .unwrap();
      logind.request_name("org.freedesktop.login1").await.unwrap();
      logind
    });
    let root = sysfs();
    state(cx, &conn, None);
    set(cx, "backlight/intel_backlight", 600).unwrap();
    let file = root
      .path()
      .join("class/backlight/intel_backlight/brightness");
    assert_eq!(fs::read_to_string(&file).unwrap(), "600");

    // and an unwritable sysfs is an error naming the file
    fs::remove_dir_all(root.path().join("class/backlight")).unwrap();
    let error = set(cx, "backlight/intel_backlight", 600).unwrap_err();
    assert!(
      format!("{error:#}").contains("intel_backlight/brightness"),
      "{error:#}"
    );
  }

  #[gpui::test]
  fn no_logind_writes_sysfs(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let conn = block_on(bus.conn());
    let root = sysfs();
    state(cx, &conn, None);
    set(cx, "backlight/intel_backlight", 700).unwrap();
    let file = root
      .path()
      .join("class/backlight/intel_backlight/brightness");
    assert_eq!(fs::read_to_string(&file).unwrap(), "700");
  }

  fn wait(cx: &mut TestAppContext, done: impl Fn(&[Display]) -> bool) -> Vec<Display> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
      cx.run_until_parked();
      let displays = cx.read(|cx| cx.brightness().list_displays(cx).to_vec());
      if done(&displays) {
        return displays;
      }
      assert!(Instant::now() < deadline, "timed out with {displays:#?}");
      std::thread::sleep(Duration::from_millis(20));
    }
  }

  fn start(cx: &mut TestAppContext, enable_ddcutil: bool) -> TestBus {
    start_on(cx, enable_ddcutil, TestBus::new())
  }

  fn start_on(cx: &mut TestAppContext, enable_ddcutil: bool, bus: TestBus) -> TestBus {
    cx.executor().allow_parking();
    let conn = block_on(bus.conn());
    let mut config = Config::default();
    config.brightness.enable_ddcutil = enable_ddcutil;
    config.brightness.poll_seconds = 1;
    cx.update(|cx| {
      cx.set_global(config);
      init(cx, &conn).unwrap();
    });
    bus
  }

  #[gpui::test]
  fn init_merges_sysfs_and_ddcutil(cx: &mut TestAppContext) {
    let root = sysfs();
    let ddcutil = FakeDdcutil::install();
    let _bus = start(cx, true);
    cx.read(|cx| {
      let brightness = cx.brightness();
      assert!(brightness.ddcutil_available && brightness.ddcutil_enabled);
    });
    let displays = wait(cx, |d| d.iter().any(|d| d.id == "ddc/5"));
    let summary: Vec<_> = displays
      .iter()
      .map(|d| (d.id.as_str(), d.brightness, d.max, d.unavailable))
      .collect();
    assert_eq!(
      summary,
      [
        ("backlight/intel_backlight", 300, 1200, None),
        ("ddc/5", 50, 100, None),
      ]
    );

    // brightness set through ddcutil shows up
    let task = cx.update(|cx| cx.brightness().set_brightness("ddc/5", 70, cx));
    block_on(task).unwrap();
    wait(cx, |d| {
      d.iter().any(|d| d.id == "ddc/5" && d.brightness == 70)
    });
    assert!(
      ddcutil
        .calls()
        .contains(&"--noconfig --bus 5 setvcp 10 70".to_string())
    );

    // sysfs changes are polled
    fs::write(
      root
        .path()
        .join("class/backlight/intel_backlight/brightness"),
      "900",
    )
    .unwrap();
    wait(cx, |d| d[0].brightness == 900);
  }

  #[gpui::test]
  fn failures_mark_the_monitor_until_it_recovers(cx: &mut TestAppContext) {
    let _root = sysfs();
    let ddcutil = FakeDdcutil::install();
    let _bus = start(cx, true);
    wait(cx, |d| d.iter().any(|d| d.id == "ddc/5"));
    ddcutil.touch("setvcp-fails-5");
    let task = cx.update(|cx| cx.brightness().set_brightness("ddc/5", 70, cx));
    block_on(task).unwrap();
    wait(cx, |d| {
      d.iter()
        .any(|d| d.id == "ddc/5" && d.unavailable == Some(Unavailable::Failed))
    });
    // quarantine ends on its own
    wait(cx, |d| {
      d.iter().any(|d| d.id == "ddc/5" && d.unavailable.is_none())
    });
  }

  #[gpui::test]
  fn ddcutil_disabled(cx: &mut TestAppContext) {
    let _root = sysfs();
    let ddcutil = FakeDdcutil::install();
    let _bus = start(cx, false);
    let displays = wait(cx, |d| d.len() == 2);
    assert_eq!(displays[1].id, "output/DP-1");
    assert_eq!(displays[1].unavailable, Some(Unavailable::DdcutilDisabled));
    assert!(ddcutil.calls().is_empty());
    cx.read(|cx| assert!(cx.brightness().ddc.is_none()));
  }

  #[gpui::test]
  fn ddcutil_missing(cx: &mut TestAppContext) {
    let _root = sysfs();
    // the daemon needs the real PATH, ddcutil must not find it
    let bus = TestBus::new();
    let empty = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("PATH", empty.path()) };
    let _bus = start_on(cx, true, bus);
    let displays = wait(cx, |d| d.len() == 2);
    assert_eq!(displays[1].unavailable, Some(Unavailable::DdcutilMissing));
    cx.read(|cx| assert!(!cx.brightness().ddcutil_available));
  }

  #[gpui::test]
  fn detect_failure(cx: &mut TestAppContext) {
    let _root = sysfs();
    let ddcutil = FakeDdcutil::install();
    ddcutil.touch("detect-fails");
    let _bus = start(cx, true);
    let displays = wait(cx, |d| {
      d.len() == 2 && d[1].unavailable == Some(Unavailable::Failed)
    });
    assert_eq!(displays[1].id, "output/DP-1");
  }
}
