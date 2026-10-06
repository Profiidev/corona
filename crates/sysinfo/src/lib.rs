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
