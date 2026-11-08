use std::time::Duration;

use anyhow::Result;
use corona_config::{Config, ConfigProvider, observe_section};
use corona_utils::Buses;
use gpui_kit::{App, AppContext, Entity, Global};

use crate::sampler::Update;

pub use crate::sampler::unique_disks;
pub use crate::state::{Disk, Gpu, GpuSample, GpuVendor, History, Sample, SystemInfo};

mod gpu;
pub mod processes;
mod sampler;
mod state;

rust_i18n::i18n!("../../assets/locales", fallback = "en");

const HISTORY: usize = 60;

#[derive(Clone)]
pub struct SystemMonitor {
  pub info: Entity<Option<SystemInfo>>,
  pub sample: Entity<Option<Sample>>,
  pub history: Entity<History>,
  pub interval: Entity<Duration>,
  intervals: flume::Sender<Duration>,
}

impl Global for SystemMonitor {}

corona_utils::global_ext!(SystemMonitorExt, system_monitor, SystemMonitor);

impl SystemMonitor {
  pub fn info<'c>(&self, cx: &'c App) -> Option<&'c SystemInfo> {
    self.info.read(cx).as_ref()
  }

  pub fn sample<'c>(&self, cx: &'c App) -> Option<&'c Sample> {
    self.sample.read(cx).as_ref()
  }

  pub fn history<'c>(&self, cx: &'c App) -> &'c History {
    self.history.read(cx)
  }

  pub fn interval(&self, cx: &App) -> Duration {
    *self.interval.read(cx)
  }

  pub fn set_interval(&self, interval: Duration, cx: &mut App) {
    self.interval.write(cx, interval);
    let _ = self.intervals.send(interval);
  }
}

fn interval(config: &Config) -> Duration {
  Duration::from_secs(config.system.monitor.poll_seconds.max(1))
}

/// Starts sampling
pub fn init(cx: &mut App, _: &Buses) -> Result<()> {
  start(cx)?;
  Ok(())
}

fn start(cx: &mut App) -> Result<()> {
  let (intervals, intervals_rx) = flume::unbounded();
  let (updates_tx, updates) = flume::unbounded();
  sampler::spawn(intervals_rx, updates_tx)?;

  let every = interval(cx.config());
  let state = SystemMonitor {
    info: cx.new(|_| None),
    sample: cx.new(|_| None),
    history: cx.new(|_| History::default()),
    interval: cx.new(|_| every),
    intervals,
  };
  state.set_interval(every, cx);

  let (info, sample, history) = (
    state.info.clone(),
    state.sample.clone(),
    state.history.clone(),
  );
  cx.spawn(async move |cx| {
    while let Ok(update) = updates.recv_async().await {
      match update {
        Update::Info(next) => info.write(cx, Some(next)),
        Update::Sample(next) => {
          history.update(cx, |history, cx| {
            history.push(&next, HISTORY);
            cx.notify();
          });
          sample.write(cx, Some(next));
        }
      }
    }
  })
  .detach();

  cx.set_global(state);
  observe_section(
    cx,
    |c| &c.system.monitor,
    |_, cx| {
      let interval = interval(cx.config());
      cx.global::<SystemMonitor>()
        .clone()
        .set_interval(interval, cx);
    },
  );
  Ok(())
}

#[cfg(test)]
mod tests {
  use std::time::Instant;

  use gpui_kit::{self as gpui, TestAppContext};

  use super::*;

  #[test]
  fn interval_is_at_least_a_second() {
    let mut config = Config::default();
    config.system.monitor.poll_seconds = 0;
    assert_eq!(interval(&config), Duration::from_secs(1));
    config.system.monitor.poll_seconds = 7;
    assert_eq!(interval(&config), Duration::from_secs(7));
  }

  /// the sampler is a real thread: wait for it in real time
  fn wait(cx: &mut TestAppContext, done: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
      cx.run_until_parked();
      if cx.read(&done) {
        return;
      }
      assert!(Instant::now() < deadline, "timed out");
      std::thread::sleep(Duration::from_millis(20));
    }
  }

  #[gpui::test]
  fn samples_into_history(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let mut config = Config::default();
    config.system.monitor.poll_seconds = 1;
    cx.update(|cx| {
      cx.set_global(config);
      super::start(cx).unwrap();
    });
    cx.read(|cx| assert_eq!(cx.system_monitor().interval(cx), Duration::from_secs(1)));
    wait(cx, |cx| cx.system_monitor().info(cx).is_some());
    wait(cx, |cx| cx.system_monitor().history(cx).cpu.len() >= 2);
    cx.read(|cx| {
      let monitor = cx.system_monitor();
      let sample = monitor.sample(cx).unwrap();
      assert!(sample.memory_total > 0);
      assert_eq!(
        monitor.history(cx).memory.len(),
        monitor.history(cx).cpu.len()
      );
    });
  }

  #[gpui::test]
  fn config_sets_the_interval(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
      cx.set_global(Config::default());
      super::start(cx).unwrap();
    });
    cx.update(|cx| {
      let mut config = cx.config().clone();
      config.system.monitor.poll_seconds = 0;
      cx.set_global(config);
    });
    cx.read(|cx| assert_eq!(cx.system_monitor().interval(cx), Duration::from_secs(1)));
  }

  #[gpui::test]
  fn rapid_config_updates(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
      cx.set_global(Config::default());
      super::start(cx).unwrap();
    });
    for sec in 1..=5 {
      cx.update(|cx| {
        let mut config = cx.config().clone();
        config.system.monitor.poll_seconds = sec;
        cx.set_global(config);
      });
    }
    cx.run_until_parked();
    cx.read(|cx| {
      assert_eq!(cx.system_monitor().interval(cx), Duration::from_secs(5));
    });
  }
}
