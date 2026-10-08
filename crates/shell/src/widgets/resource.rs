use corona_surface::{
  bar::{Button, Widget},
  tooltip::TooltipExt,
};
use corona_sysinfo::{History, Sample, SystemMonitorExt};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  AnyWindowHandle, Bounds, Context, InteractiveElement, IntoElement, ParentElement, Pixels, Render,
  StatefulInteractiveElement, Styled, Subscription, Task, Window,
  assets::IconName,
  base::ElementExt,
  component::{ActiveTheme, Icon},
  div,
  prelude::FluentBuilder,
  px, relative,
};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, cell::Cell, rc::Rc, time::Duration};
use uuid::Uuid;

use crate::{
  control_center::{Standalone, SysinfoPanel, sysinfo::details::rate},
  i18n::decimal,
  widgets::tooltip::TextTooltip,
};
use rust_i18n::t;

const METER_HEIGHT: f32 = 14.;
const METER_WIDTH: f32 = 3.;
/// network meters are relative to the busiest recent second
const MIN_RATE: f64 = 1024. * 1024.;
const TOOLTIP_DELAY: Duration = Duration::from_millis(200);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
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
  pub(crate) const ALL: [Stat; 11] = [
    Stat::Cpu,
    Stat::Load,
    Stat::Temperature,
    Stat::Memory,
    Stat::Swap,
    Stat::Gpu,
    Stat::GpuTemperature,
    Stat::Vram,
    Stat::Disk,
    Stat::Download,
    Stat::Upload,
  ];

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

  pub(crate) fn thresholds(self) -> Option<(f32, f32)> {
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

  /// The reading as text, like `42%` or `1,2 MB/s`
  fn value(self, sample: &Sample, history: &History, mount: &str) -> Option<String> {
    match self {
      Stat::Load => Some(decimal(sample.load[0], 2)),
      Stat::Temperature | Stat::GpuTemperature => {
        Some(format!("{:.0}°C", self.level(sample, history, mount)?))
      }
      Stat::Download => Some(rate(sample.network_rx)),
      Stat::Upload => Some(rate(sample.network_tx)),
      _ => Some(format!("{:.0}%", self.level(sample, history, mount)?)),
    }
  }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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
  bounds: Rc<Cell<Bounds<Pixels>>>,
  tooltip: Option<AnyWindowHandle>,
  hover: Option<Task<()>>,
  _subscription: Subscription,
}

impl Resource {
  fn tooltip_text(&self, cx: &Context<Self>) -> String {
    let stat = self.options.stat;
    let monitor = cx.system_monitor();
    let value = monitor
      .sample(cx)
      .and_then(|s| stat.value(s, monitor.history(cx), &self.options.mount))
      .unwrap_or_else(|| "–".into());
    format!("{}: {value}", stat_name(stat))
  }

  fn show_tooltip(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let text = self.tooltip_text(cx);
    if cx
      .show_bar_tooltip(TextTooltip::new(text), self.bounds.get(), window)
      .log_err()
      .is_ok()
    {
      self.tooltip = Some(window.window_handle());
    }
  }
}

impl Widget for Resource {
  const NAME: &'static str = "resource";

  type Options = Options;

  fn init(cx: &mut Context<'_, Self>, _display_id: Uuid, options: Options) -> Self {
    let sample = cx.system_monitor().sample.clone();
    Self {
      options,
      bounds: Rc::default(),
      tooltip: None,
      hover: None,
      _subscription: cx.observe(&sample, |this, _, cx| {
        if let Some(handle) = this.tooltip {
          let (text, bounds) = (this.tooltip_text(cx), this.bounds.get());
          let _ = handle.update(cx, |_, window, cx| {
            cx.show_bar_tooltip(TextTooltip::new(text), bounds, window)
              .log_err()
              .ok();
          });
        }
        cx.notify();
      }),
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

    let bounds = self.bounds.clone();

    div()
      .id("resource-hover")
      .on_prepaint(move |b, _, _| bounds.set(b))
      .on_hover(cx.listener(|this, hovered: &bool, window, cx| {
        if !*hovered {
          this.tooltip = None;
          this.hover = None;
          cx.hide_tooltip::<TextTooltip>();
          return;
        }

        this.hover = Some(cx.spawn_in(window, async move |e, cx| {
          cx.background_executor().timer(TOOLTIP_DELAY).await;
          e.update_in(cx, |this, window, cx| this.show_tooltip(window, cx))
            .ok();
        }));
      }))
      .child(
        Button::<_, Standalone<SysinfoPanel>>::new(
          cx,
          "resource",
          Icon::new(stat.icon()).when_some(alert, |icon, color| icon.text_color(color)),
        )
        .suffix(meter),
      )
  }
}

/// A stat as people say it, like "GPU temperature"
pub(crate) fn stat_name(stat: Stat) -> Cow<'static, str> {
  match stat {
    Stat::Cpu => t!("app.sysinfo.cpu"),
    Stat::Load => t!("app.resource.load"),
    Stat::Temperature => t!("app.resource.cpu_temperature"),
    Stat::Memory => t!("app.sysinfo.memory"),
    Stat::Swap => t!("app.resource.swap"),
    Stat::Gpu => t!("app.sysinfo.gpu"),
    Stat::GpuTemperature => t!("app.resource.gpu_temperature"),
    Stat::Vram => "VRAM".into(),
    Stat::Disk => t!("app.resource.disk"),
    Stat::Download => t!("app.sysinfo.download"),
    Stat::Upload => t!("app.sysinfo.upload"),
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

  #[test]
  fn values() {
    let sample = Sample {
      time: std::time::Instant::now(),
      cpu: 42.4,
      cpu_cores: vec![0.; 4],
      cpu_frequency: 0,
      cpu_temperature: None,
      memory_used: 1,
      memory_total: 4,
      swap_used: 0,
      swap_total: 0,
      load: [1.5, 0., 0.],
      network_rx: 2.5e6,
      network_tx: 0.,
      disks: vec![],
      gpus: vec![],
    };
    let history = History::default();
    let value = |stat: Stat| stat.value(&sample, &history, "/");

    assert_eq!(value(Stat::Cpu).as_deref(), Some("42%"));
    assert_eq!(value(Stat::Memory).as_deref(), Some("25%"));
    assert_eq!(value(Stat::Temperature), None);
    assert_eq!(value(Stat::Swap), None);
    assert!(value(Stat::Load).is_some_and(|v| v.starts_with('1')));
    assert!(value(Stat::Download).is_some_and(|v| v.ends_with("MB/s")));
  }
}
