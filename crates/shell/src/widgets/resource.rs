use corona_surface::bar::{Button, Widget};
use corona_sysinfo::{History, Sample, SystemMonitorExt};
use gpui_kit::{
  Context, IntoElement, ParentElement, Render, Styled, Subscription, Window,
  assets::IconName,
  component::{ActiveTheme, Icon},
  div,
  prelude::FluentBuilder,
  px, relative,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::control_center::{Standalone, SysinfoPanel};

const METER_HEIGHT: f32 = 14.;
const METER_WIDTH: f32 = 3.;
/// network meters are relative to the busiest recent second
const MIN_RATE: f64 = 1024. * 1024.;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stat {
  #[default]
  Cpu,
  Load,
  Temperature,
  Memory,
  Swap,
  Gpu,
  GpuTemperature,
  Vram,
  Disk,
  Download,
  Upload,
}

impl Stat {
  fn icon(self) -> IconName {
    match self {
      Stat::Cpu => IconName::Gauge,
      Stat::Load => IconName::Activity,
      Stat::Temperature => IconName::Flame,
      Stat::Memory => IconName::Cpu,
      Stat::Swap => IconName::ArrowLeftRight,
      Stat::Gpu => IconName::Gpu,
      Stat::GpuTemperature => IconName::Thermometer,
      Stat::Vram => IconName::Microchip,
      Stat::Disk => IconName::Database,
      Stat::Download => IconName::Download,
      Stat::Upload => IconName::Upload,
    }
  }

  fn thresholds(self) -> Option<(f32, f32)> {
    match self {
      Stat::Cpu => Some((70., 90.)),
      Stat::Load => Some((70., 100.)),
      Stat::Temperature => Some((70., 85.)),
      Stat::Memory | Stat::Vram => Some((80., 95.)),
      Stat::Swap => Some((50., 80.)),
      Stat::Gpu => Some((80., 95.)),
      Stat::GpuTemperature => Some((75., 90.)),
      Stat::Disk => Some((80., 95.)),
      Stat::Download | Stat::Upload => None,
    }
  }

  fn level(self, sample: &Sample, history: &History, mount: &str) -> Option<f32> {
    let percent = |used: u64, total: u64| (total > 0).then(|| used as f32 / total as f32 * 100.);
    let rate = |now: f64, recent: &std::collections::VecDeque<f64>| {
      let peak = recent.iter().copied().fold(MIN_RATE, f64::max);
      Some((now / peak * 100.) as f32)
    };
    let gpu = sample.gpus.first();

    match self {
      Stat::Cpu => Some(sample.cpu),
      Stat::Load => {
        let cores = sample.cpu_cores.len().max(1);
        Some((sample.load[0] / cores as f64 * 100.) as f32)
      }
      Stat::Temperature => sample.cpu_temperature,
      Stat::Memory => percent(sample.memory_used, sample.memory_total),
      Stat::Swap => percent(sample.swap_used, sample.swap_total),
      Stat::Gpu => gpu?.usage,
      Stat::GpuTemperature => gpu?.temperature,
      Stat::Vram => percent(gpu?.vram_used?, gpu?.vram_total?),
      Stat::Disk => {
        let disk = sample.disks.iter().find(|d| d.mount_point == mount)?;
        percent(disk.total - disk.available, disk.total)
      }
      Stat::Download => rate(sample.network_rx, &history.network_rx),
      Stat::Upload => rate(sample.network_tx, &history.network_tx),
    }
  }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct Options {
  pub stat: Stat,
  pub warning: Option<f32>,
  pub critical: Option<f32>,
  pub mount: String,
}

impl Default for Options {
  fn default() -> Self {
    Self {
      stat: Stat::default(),
      warning: None,
      critical: None,
      mount: "/".to_string(),
    }
  }
}

pub struct Resource {
  options: Options,
  _subscription: Subscription,
}

impl Widget for Resource {
  const NAME: &'static str = "resource";

  type Options = Options;

  fn init(cx: &mut Context<'_, Self>, _display_id: Uuid, options: Options) -> Self {
    let sample = cx.system_monitor().sample.clone();
    Self {
      options,
      _subscription: cx.observe(&sample, |_, _, cx| cx.notify()),
    }
  }
}

impl Render for Resource {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let Options {
      stat,
      warning,
      critical,
      ..
    } = self.options;
    let monitor = cx.system_monitor();
    let level = monitor
      .sample(cx)
      .and_then(|s| stat.level(s, monitor.history(cx), &self.options.mount))
      .unwrap_or(0.);

    let theme = cx.theme();
    let defaults = stat.thresholds();
    let critical = critical.or(defaults.map(|d| d.1));
    let warning = warning.or(defaults.map(|d| d.0));
    let alert = if critical.is_some_and(|c| level >= c) {
      Some(theme.danger)
    } else if warning.is_some_and(|w| level >= w) {
      Some(theme.warning)
    } else {
      None
    };
    let (track, fill) = (theme.muted, alert.unwrap_or(theme.primary));

    let meter = div()
      .relative()
      .w(px(METER_WIDTH))
      .h(px(METER_HEIGHT))
      .rounded_full()
      .bg(track)
      .child(
        div()
          .absolute()
          .bottom_0()
          .w_full()
          .h(relative((level / 100.).clamp(0., 1.)))
          .rounded_full()
          .bg(fill),
      );

    Button::<_, Standalone<SysinfoPanel>>::new(
      cx,
      "resource",
      Icon::new(stat.icon()).when_some(alert, |icon, color| icon.text_color(color)),
    )
    .suffix(meter)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn options_parse() {
    let options: Options =
      serde_json::from_value(serde_json::json!({ "stat": "gpu_temperature", "critical": 80 }))
        .unwrap();
    assert_eq!(options.stat, Stat::GpuTemperature);
    assert_eq!(options.critical, Some(80.));
    assert_eq!(options.mount, "/");
  }
}
