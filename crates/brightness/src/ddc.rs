use std::{
  collections::HashMap,
  env,
  io::Read,
  process::{Command, Stdio},
  thread,
  time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};

use crate::state::{Display, DisplayKind};

/// the brightness VCP feature
const BRIGHTNESS: &str = "10";
#[cfg(not(test))]
const TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(not(test))]
const QUARANTINE: Duration = Duration::from_secs(5 * 60);
#[cfg(not(test))]
const STARTUP_DELAY: Duration = Duration::from_secs(3);
#[cfg(test)]
const TIMEOUT: Duration = Duration::from_millis(500);
#[cfg(test)]
const QUARANTINE: Duration = Duration::from_millis(400);
#[cfg(test)]
const STARTUP_DELAY: Duration = Duration::ZERO;

pub(crate) enum DdcCommand {
  Set { bus: u32, brightness: u32 },
}

pub(crate) enum Update {
  Displays(Vec<Display>),
  DetectFailed,
  Brightness { bus: u32, brightness: u32 },
  Failed { bus: u32 },
  Recovered { bus: u32 },
}

pub fn available() -> bool {
  env::var_os("PATH")
    .is_some_and(|path| env::split_paths(&path).any(|dir| dir.join("ddcutil").is_file()))
}

pub(crate) fn worker(commands: flume::Receiver<DdcCommand>, updates: flume::Sender<Update>) {
  thread::spawn(move || {
    // probing over the NVIDIA I2C bus stalls the GPU setup of the first frame
    thread::sleep(STARTUP_DELAY);
    let detected = match detect() {
      Ok(displays) => Update::Displays(displays),
      Err(e) => {
        tracing::warn!("ddcutil detect failed: {e:?}");
        Update::DetectFailed
      }
    };
    if updates.send(detected).is_err() {
      return;
    }

    let mut quarantined: HashMap<u32, Instant> = HashMap::new();
    loop {
      let next = quarantined.values().min().copied();
      let first = match next {
        Some(until) => commands.recv_deadline(until),
        None => commands
          .recv()
          .map_err(|_| flume::RecvTimeoutError::Disconnected),
      };
      let first = match first {
        Ok(first) => first,
        Err(flume::RecvTimeoutError::Timeout) => {
          let now = Instant::now();
          let ended: Vec<u32> = quarantined
            .iter()
            .filter(|(_, until)| **until <= now)
            .map(|(bus, _)| *bus)
            .collect();
          for bus in ended {
            quarantined.remove(&bus);
            if updates.send(Update::Recovered { bus }).is_err() {
              return;
            }
          }
          continue;
        }
        Err(flume::RecvTimeoutError::Disconnected) => return,
      };
      let mut latest: HashMap<u32, u32> = HashMap::new();
      for DdcCommand::Set { bus, brightness } in std::iter::once(first).chain(commands.try_iter()) {
        latest.insert(bus, brightness);
      }
      for (bus, brightness) in latest {
        if quarantined
          .get(&bus)
          .is_some_and(|until| Instant::now() < *until)
        {
          continue;
        }
        let set = run(&[
          "--bus",
          &bus.to_string(),
          "setvcp",
          BRIGHTNESS,
          &brightness.to_string(),
        ]);
        match set {
          Ok(_) => {
            quarantined.remove(&bus);
            if updates
              .send(Update::Brightness { bus, brightness })
              .is_err()
            {
              return;
            }
          }
          Err(e) => {
            tracing::warn!("ddcutil setvcp on bus {bus} failed, pausing it: {e:?}");
            quarantined.insert(bus, Instant::now() + QUARANTINE);
            if updates.send(Update::Failed { bus }).is_err() {
              return;
            }
          }
        }
      }
    }
  });
}

fn detect() -> Result<Vec<Display>> {
  let mut displays = Vec::new();
  for found in parse_detect(&run(&["detect"])?) {
    match run(&[
      "--bus",
      &found.bus.to_string(),
      "getvcp",
      BRIGHTNESS,
      "--brief",
    ])
    .and_then(|out| parse_getvcp(&out))
    {
      Ok((brightness, max)) => displays.push(Display {
        id: format!("ddc/{}", found.bus),
        output: found.output,
        name: found.model,
        kind: DisplayKind::External,
        brightness,
        max,
        unavailable: None,
      }),
      // monitors without DDC/CI or with it turned off in their menu
      Err(e) => tracing::debug!("bus {} has no brightness: {e:?}", found.bus),
    }
  }
  Ok(displays)
}

fn run(args: &[&str]) -> Result<String> {
  let mut child = Command::new("ddcutil")
    .arg("--noconfig")
    .args(args)
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .spawn()
    .context("running ddcutil")?;
  let deadline = Instant::now() + TIMEOUT;
  loop {
    if let Some(status) = child.try_wait()? {
      let mut out = String::new();
      child
        .stdout
        .take()
        .context("ddcutil stdout")?
        .read_to_string(&mut out)?;
      if !status.success() {
        bail!("ddcutil {} exited with {status}", args.join(" "));
      }
      return Ok(out);
    }
    if Instant::now() > deadline {
      child.kill().ok();
      child.wait().ok();
      bail!("ddcutil {} timed out", args.join(" "));
    }
    thread::sleep(Duration::from_millis(20));
  }
}

#[derive(Debug, PartialEq)]
struct Detected {
  bus: u32,
  output: Option<String>,
  model: Option<String>,
}

fn parse_detect(out: &str) -> Vec<Detected> {
  let mut found = Vec::new();
  let mut current: Option<Detected> = None;
  for line in out.lines() {
    if !line.starts_with(char::is_whitespace) {
      found.extend(current.take());
      if line.starts_with("Display ") {
        current = Some(Detected {
          bus: 0,
          output: None,
          model: None,
        });
      }
      continue;
    }
    let (Some(display), Some((key, value))) = (current.as_mut(), line.trim().split_once(':'))
    else {
      continue;
    };
    let value = value.trim();
    match key.trim() {
      "I2C bus" => {
        display.bus = value
          .strip_prefix("/dev/i2c-")
          .and_then(|bus| bus.parse().ok())
          .unwrap_or_default()
      }
      // `card1-DP-1`, older versions write `DRM connector`
      "DRM_connector" | "DRM connector" => {
        display.output = value.split_once('-').map(|(_, output)| output.to_string())
      }
      "Model" if !value.is_empty() => display.model = Some(value.to_string()),
      _ => {}
    }
  }
  found.extend(current);
  found.retain(|d| d.bus != 0);
  found
}

/// `VCP 10 C 50 100`: the current and the maximum value
fn parse_getvcp(out: &str) -> Result<(u32, u32)> {
  let fields: Vec<&str> = out.split_whitespace().collect();
  match fields[..] {
    ["VCP", _, "C", current, max, ..] => Ok((current.parse()?, max.parse()?)),
    _ => bail!("unexpected getvcp output: {}", out.trim()),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const DETECT: &str = "\
Display 1
   I2C bus:  /dev/i2c-5
   DRM_connector:           card1-DP-1
   EDID synopsis:
      Mfg id:               DEL - Dell Inc.
      Model:                DELL U2720Q
      Serial number:        ABC123
   VCP version:         2.1

Invalid display
   I2C bus:  /dev/i2c-7
   DRM_connector:           card1-HDMI-A-1

Display 2
   I2C bus:  /dev/i2c-6
   DRM connector:           card1-DP-2
   EDID synopsis:
      Model:                LG ULTRAGEAR
";

  #[test]
  fn detect() {
    assert_eq!(
      parse_detect(DETECT),
      [
        Detected {
          bus: 5,
          output: Some("DP-1".into()),
          model: Some("DELL U2720Q".into()),
        },
        Detected {
          bus: 6,
          output: Some("DP-2".into()),
          model: Some("LG ULTRAGEAR".into()),
        },
      ]
    );
  }

  #[test]
  fn getvcp() {
    assert_eq!(parse_getvcp("VCP 10 C 50 100\n").unwrap(), (50, 100));
    assert!(parse_getvcp("VCP 10 ERR\n").is_err());
  }

  use crate::fake_ddcutil::FakeDdcutil;

  #[test]
  fn detect_edges() {
    let out = "\
ddcutil noise before any display
Display 1\r
   I2C bus:  /dev/i2c-3\r
   DRM_connector:           card0-eDP-1\r
   EDID synopsis:\r
      Model:                \r
Display 2
   DRM_connector:           card0-DP-3
Display 3
   I2C bus:  /dev/i2c-x
Display 4
   I2C bus:  /dev/i2c-9
   DRM_connector:           nodash
   Model: Odd: Name";
    assert_eq!(
      parse_detect(out),
      [
        Detected {
          bus: 3,
          output: Some("eDP-1".into()),
          model: None,
        },
        Detected {
          bus: 9,
          output: None,
          model: Some("Odd: Name".into()),
        },
      ]
    );
    assert!(parse_detect("").is_empty());
    assert!(parse_detect("No displays found.\n").is_empty());
  }

  #[test]
  fn getvcp_edges() {
    assert_eq!(parse_getvcp("  VCP 10 C 0 0 extra\n").unwrap(), (0, 0));
    for bad in [
      "",
      "VCP 10 C 50",
      "VCP 10 C x 100",
      "VCP 10 C 50 -1",
      "VCP 10 SNC x01",
    ] {
      assert!(parse_getvcp(bad).is_err(), "{bad}");
    }
    assert_eq!(
      parse_getvcp("VCP 10 ERR").unwrap_err().to_string(),
      "unexpected getvcp output: VCP 10 ERR"
    );
  }

  #[test]
  fn available_needs_a_file_on_path() {
    let dir = tempfile::tempdir().unwrap();
    unsafe { env::set_var("PATH", dir.path()) };
    assert!(!available());
    std::fs::create_dir(dir.path().join("ddcutil")).unwrap();
    assert!(!available());
    let other = tempfile::tempdir().unwrap();
    std::fs::write(other.path().join("ddcutil"), "").unwrap();
    unsafe { env::set_var("PATH", env::join_paths([dir.path(), other.path()]).unwrap()) };
    assert!(available());
    unsafe { env::remove_var("PATH") };
    assert!(!available());
  }

  #[test]
  fn run_outcomes() {
    let fake = FakeDdcutil::install();
    assert_eq!(run(&["detect"]).unwrap(), crate::fake_ddcutil::DETECT);
    assert_eq!(fake.calls(), ["--noconfig detect"]);
    let error = run(&["--bus", "6", "getvcp", BRIGHTNESS, "--brief"]).unwrap_err();
    assert_eq!(
      error.to_string(),
      "ddcutil --bus 6 getvcp 10 --brief exited with exit status: 1"
    );
    fake.touch("hang");
    let started = Instant::now();
    let error = run(&["--bus", "5", "setvcp", BRIGHTNESS, "1"]).unwrap_err();
    assert!(error.to_string().ends_with("timed out"), "{error}");
    assert!(started.elapsed() < TIMEOUT * 4);
  }

  #[test]
  fn run_without_ddcutil() {
    let empty = tempfile::tempdir().unwrap();
    unsafe { env::set_var("PATH", empty.path()) };
    assert_eq!(run(&["detect"]).unwrap_err().to_string(), "running ddcutil");
  }

  #[test]
  fn detect_reads_each_bus() {
    let fake = FakeDdcutil::install();
    let displays = super::detect().unwrap();
    // bus 6 has no brightness and is left out
    assert_eq!(
      displays,
      [Display {
        id: "ddc/5".into(),
        output: Some("DP-1".into()),
        name: Some("DELL U2720Q".into()),
        kind: DisplayKind::External,
        brightness: 50,
        max: 100,
        unavailable: None,
      }]
    );
    assert_eq!(
      fake.calls(),
      [
        "--noconfig detect",
        "--noconfig --bus 5 getvcp 10 --brief",
        "--noconfig --bus 6 getvcp 10 --brief",
      ]
    );
  }

  fn next(updates: &flume::Receiver<Update>) -> Update {
    updates
      .recv_timeout(Duration::from_secs(10))
      .expect("an update")
  }

  #[test]
  fn worker_detects_then_sets() {
    let fake = FakeDdcutil::install();
    let (commands, commands_rx) = flume::unbounded();
    let (updates_tx, updates) = flume::unbounded();
    // queued while detecting: only the latest value per bus is applied
    for brightness in [10, 20, 30] {
      commands
        .send(DdcCommand::Set { bus: 5, brightness })
        .unwrap();
    }
    worker(commands_rx, updates_tx);
    assert!(matches!(next(&updates), Update::Displays(d) if d.len() == 1));
    assert!(matches!(
      next(&updates),
      Update::Brightness {
        bus: 5,
        brightness: 30
      }
    ));
    let sets: Vec<_> = fake
      .calls()
      .into_iter()
      .filter(|c| c.contains("setvcp"))
      .collect();
    assert_eq!(sets, ["--noconfig --bus 5 setvcp 10 30"]);

    drop(commands);
    assert!(matches!(
      updates.recv_timeout(Duration::from_secs(10)),
      Err(flume::RecvTimeoutError::Disconnected)
    ));
  }

  #[test]
  fn worker_reports_a_failed_detect() {
    let fake = FakeDdcutil::install();
    fake.touch("detect-fails");
    let (_commands, commands_rx) = flume::unbounded();
    let (updates_tx, updates) = flume::unbounded();
    worker(commands_rx, updates_tx);
    assert!(matches!(next(&updates), Update::DetectFailed));
  }

  #[test]
  fn worker_quarantines_failing_buses() {
    let fake = FakeDdcutil::install();
    fake.touch("setvcp-fails-5");
    let (commands, commands_rx) = flume::unbounded();
    let (updates_tx, updates) = flume::unbounded();
    worker(commands_rx, updates_tx);
    next(&updates);
    commands
      .send(DdcCommand::Set {
        bus: 5,
        brightness: 1,
      })
      .unwrap();
    assert!(matches!(next(&updates), Update::Failed { bus: 5 }));

    // ignored while quarantined, other buses still work
    std::fs::remove_file(fake.file("setvcp-fails-5")).unwrap();
    commands
      .send(DdcCommand::Set {
        bus: 5,
        brightness: 2,
      })
      .unwrap();
    commands
      .send(DdcCommand::Set {
        bus: 7,
        brightness: 3,
      })
      .unwrap();
    assert!(matches!(
      next(&updates),
      Update::Brightness {
        bus: 7,
        brightness: 3
      }
    ));
    assert!(
      !fake
        .calls()
        .contains(&"--noconfig --bus 5 setvcp 10 2".to_string())
    );

    assert!(matches!(next(&updates), Update::Recovered { bus: 5 }));
    commands
      .send(DdcCommand::Set {
        bus: 5,
        brightness: 4,
      })
      .unwrap();
    assert!(matches!(
      next(&updates),
      Update::Brightness {
        bus: 5,
        brightness: 4
      }
    ));
  }

  #[test]
  fn worker_stops_without_listeners() {
    let _fake = FakeDdcutil::install();
    let (commands, commands_rx) = flume::unbounded();
    let (updates_tx, updates) = flume::unbounded::<Update>();
    drop(updates);
    worker(commands_rx, updates_tx);
    let deadline = Instant::now() + Duration::from_secs(10);
    while commands
      .send(DdcCommand::Set {
        bus: 5,
        brightness: 1,
      })
      .is_ok()
    {
      assert!(Instant::now() < deadline, "worker kept running");
      thread::sleep(Duration::from_millis(10));
    }
  }
}
