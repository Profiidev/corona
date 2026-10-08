use std::time::Duration;

use corona_config::{Config, ConfigProvider, observe_section};
use gpui_kit::{App, AppContext, Entity, Global};

use crate::sampler::Update;

pub use crate::state::{Disk, Gpu, GpuSample, GpuVendor, History, Sample, SystemInfo};

mod gpu;
mod sampler;
mod state;

const HISTORY: usize = 60;

#[derive(Clone)]
pub struct SystemMonitor {
  pub info: Entity<Option<SystemInfo>>,
  pub sample: Entity<Option<Sample>>,
  pub history: Entity<History>,
  pub interval: Entity<Option<Duration>>,
  intervals: flume::Sender<Option<Duration>>,
}

impl Global for SystemMonitor {}

pub trait SystemMonitorExt {
  fn system_monitor(&self) -> &SystemMonitor;
}

impl SystemMonitorExt for App {
  fn system_monitor(&self) -> &SystemMonitor {
    self.global::<SystemMonitor>()
  }
}

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

  pub fn interval(&self, cx: &App) -> Option<Duration> {
    *self.interval.read(cx)
  }

  pub fn set_interval(&self, interval: Option<Duration>, cx: &mut App) {
    self.interval.write(cx, interval);
    let _ = self.intervals.send(interval);
  }
}

fn interval(config: &Config) -> Duration {
  Duration::from_secs(config.system.monitor.poll_seconds.max(1))
}

pub fn init(cx: &mut App) {
  let (intervals, intervals_rx) = flume::unbounded();
  let (updates_tx, updates) = flume::unbounded();
  sampler::spawn(intervals_rx, updates_tx);

  let state = SystemMonitor {
    info: cx.new(|_| None),
    sample: cx.new(|_| None),
    history: cx.new(|_| History::default()),
    interval: cx.new(|_| None),
    intervals,
  };
  state.set_interval(Some(interval(cx.config())), cx);

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
        .set_interval(Some(interval), cx);
    },
  );
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
      init(cx);
    });
    cx.read(|cx| {
      assert_eq!(
        cx.system_monitor().interval(cx),
        Some(Duration::from_secs(1))
      )
    });
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
  fn config_and_pausing_set_the_interval(cx: &mut TestAppContext) {
    cx.update(|cx| {
      cx.set_global(Config::default());
      init(cx);
    });
    cx.update(|cx| {
      let mut config = cx.config().clone();
      config.system.monitor.poll_seconds = 0;
      cx.set_global(config);
    });
    cx.read(|cx| {
      assert_eq!(
        cx.system_monitor().interval(cx),
        Some(Duration::from_secs(1))
      )
    });
    cx.update(|cx| cx.system_monitor().clone().set_interval(None, cx));
    cx.read(|cx| assert_eq!(cx.system_monitor().interval(cx), None));
  }

  #[gpui::test]
  fn rapid_config_updates(cx: &mut TestAppContext) {
    cx.update(|cx| {
      cx.set_global(Config::default());
      init(cx);
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
      assert_eq!(
        cx.system_monitor().interval(cx),
        Some(Duration::from_secs(5))
      );
    });
  }

  #[gpui::test]
  fn background_update_listener_task_terminates_on_disconnect(cx: &mut TestAppContext) {
    cx.update(|cx| {
      cx.set_global(Config::default());
      init(cx);
    });
    // Task spawned in init runs and stays idle waiting for updates or terminates when disconnected
    cx.run_until_parked();
    let (tx, rx) = flume::unbounded::<Update>();
    drop(tx);
    assert!(rx.recv().is_err());
  }
}
