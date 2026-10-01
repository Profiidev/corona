use std::time::{Duration, SystemTime};

use corona_sysinfo::{GpuVendor, SystemMonitorExt};
use gpui_kit::{
  Context, IntoElement, ParentElement, Styled,
  assets::IconName,
  base::StyledExt,
  component::{Icon, Sizable, Theme},
  div, px,
};

use crate::control_center::sysinfo::{SysinfoPanel, card};

const DETAIL_TEXT: f32 = 10.;
const DETAIL_LINE: f32 = 14.;

const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];

fn size(bytes: u64) -> String {
  let mut value = bytes as f64;
  let mut unit = 0;
  while value >= 1024. && unit < UNITS.len() - 1 {
    value /= 1024.;
    unit += 1;
  }
  format!("{value:.1} {}", UNITS[unit])
}

pub(super) fn rate(bytes_per_second: f64) -> String {
  match bytes_per_second {
    r if r >= 1e6 => format!("{:.1} MB/s", r / 1e6),
    r => format!("{:.1} kB/s", r / 1e3),
  }
}

fn duration(duration: Duration) -> String {
  const UNITS: [(u64, &str); 6] = [
    (365 * 86400, "year"),
    (30 * 86400, "month"),
    (86400, "day"),
    (3600, "hour"),
    (60, "minute"),
    (1, "second"),
  ];
  let mut seconds = duration.as_secs();
  let parts: Vec<String> = UNITS
    .iter()
    .filter_map(|(size, name)| {
      let count = seconds / size;
      seconds %= size;
      (count > 0).then(|| format!("{count} {name}{}", if count == 1 { "" } else { "s" }))
    })
    .take(2)
    .collect();
  if parts.is_empty() {
    "0 seconds".into()
  } else {
    parts.join(" ")
  }
}

fn since(time: SystemTime) -> Duration {
  time.elapsed().unwrap_or_default()
}

fn line(theme: &Theme, icon: IconName, text: String) -> impl IntoElement {
  div()
    .flex()
    .gap_2()
    .items_center()
    .text_size(px(DETAIL_TEXT))
    .line_height(px(DETAIL_LINE))
    .text_color(theme.colors.muted_foreground)
    .child(Icon::new(icon).xsmall())
    .child(div().flex_1().min_w_0().truncate().child(text))
}

fn value(theme: &Theme, icon: IconName, label: String, value: String) -> impl IntoElement {
  div()
    .flex()
    .gap_2()
    .items_center()
    .text_size(px(DETAIL_TEXT))
    .line_height(px(DETAIL_LINE))
    .child(Icon::new(icon).xsmall())
    .child(
      div()
        .flex_1()
        .min_w_0()
        .truncate()
        .text_color(theme.colors.muted_foreground)
        .child(label),
    )
    .child(value)
}

impl SysinfoPanel {
  pub(super) fn system(&self, theme: &Theme, cx: &Context<'_, Self>) -> impl IntoElement {
    let mut system = card(theme)
      .gap_1()
      .child(div().text_sm().font_bold().child("System"));
    let Some(info) = cx.system_monitor().info(cx) else {
      return system;
    };

    system = system.child(line(theme, IconName::Cpu, info.cpu_model.clone()));
    for gpu in &info.gpus {
      let name = match &gpu.driver {
        Some(driver) => format!("{} ({driver})", gpu.name),
        None => gpu.name.clone(),
      };
      let icon = match gpu.vendor {
        GpuVendor::Nvidia | GpuVendor::Amd => IconName::Gpu,
        GpuVendor::Intel | GpuVendor::Other => IconName::Monitor,
      };
      system = system.child(line(theme, icon, name));
    }
    system = system
      .child(line(theme, IconName::Monitor, info.os.clone()))
      .child(line(
        theme,
        IconName::Layers,
        format!("Linux {}", info.kernel),
      ));
    if let Some(compositor) = &info.compositor {
      system = system.child(line(theme, IconName::AppWindow, compositor.clone()));
    }
    let uptime = format!("Up {}", duration(since(info.booted)));
    system.child(line(theme, IconName::Clock, uptime))
  }

  pub(super) fn resources(&self, theme: &Theme, cx: &Context<'_, Self>) -> impl IntoElement {
    let mut resources = card(theme)
      .gap_1()
      .child(div().text_sm().font_bold().child("Resources"));
    let monitor = cx.system_monitor();
    let Some(sample) = monitor.sample(cx) else {
      return resources;
    };

    let [one, five, fifteen] = sample.load;
    resources = resources
      .child(value(
        theme,
        IconName::Activity,
        "CPU load".into(),
        format!("{one:.2} / {five:.2} / {fifteen:.2}"),
      ))
      .child(value(
        theme,
        IconName::MemoryStick,
        "RAM".into(),
        format!(
          "{} / {}",
          size(sample.memory_used),
          size(sample.memory_total)
        ),
      ))
      .child(value(
        theme,
        IconName::ArrowLeftRight,
        "Swap".into(),
        format!("{} / {}", size(sample.swap_used), size(sample.swap_total)),
      ));

    for disk in &sample.disks {
      let used = disk.total.saturating_sub(disk.available);
      let percent = used as f64 / disk.total.max(1) as f64 * 100.;
      resources = resources.child(value(
        theme,
        IconName::HardDrive,
        disk.mount_point.clone(),
        format!("{} / {} ({percent:.0}%)", size(used), size(disk.total)),
      ));
    }
    resources
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn formatting() {
    assert_eq!(size(24_266_000_000), "22.6 GiB");
    assert_eq!(rate(15_900.), "15.9 kB/s");
    assert_eq!(rate(2_500_000.), "2.5 MB/s");
    assert_eq!(
      duration(Duration::from_secs(12 * 3600 + 60 + 5)),
      "12 hours 1 minute"
    );
    assert_eq!(
      duration(Duration::from_secs(
        365 * 86400 + 4 * 30 * 86400 + 3 * 86400
      )),
      "1 year 4 months"
    );
    assert_eq!(duration(Duration::ZERO), "0 seconds");
  }
}
