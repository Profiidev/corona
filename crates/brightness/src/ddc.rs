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
const TIMEOUT: Duration = Duration::from_secs(5);
const QUARANTINE: Duration = Duration::from_secs(5 * 60);
const STARTUP_DELAY: Duration = Duration::from_secs(3);

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
        name: found.model.unwrap_or_else(|| "External display".into()),
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
}
