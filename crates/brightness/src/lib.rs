use std::{
  collections::HashSet,
  path::Path,
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

  let monitors = outputs.iter().filter(|o| !builtin(o)).map(|output| {
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
  });
  // monitors `ddcutil` could not tie to an output still work
  let unmatched = detected
    .iter()
    .filter(|d| {
      !d.output
        .as_ref()
        .is_some_and(|o| outputs.iter().any(|out| &out.name == o))
    })
    .cloned()
    .map(mark_failed);

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
      let drm = Path::new(backlight::DRM);
      let current = (
        backlight::list(Path::new(backlight::BACKLIGHT), drm),
        outputs::connected(drm),
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
}
