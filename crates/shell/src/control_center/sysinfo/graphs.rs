use std::collections::VecDeque;

use corona_sysinfo::SystemMonitorExt;
use gpui_kit::{
  Background, Context, Hsla, IntoElement, ParentElement, SharedString, Styled,
  assets::IconName,
  base::StyledExt,
  component::{Icon, Sizable, Theme, chart::AreaChart},
  div, linear_color_stop, linear_gradient,
  prelude::FluentBuilder,
  px,
};

use crate::control_center::sysinfo::{SysinfoPanel, card, details::rate};

const MIN_GRAPH: f32 = 40.;

#[derive(Clone)]
struct Point {
  x: SharedString,
  values: Vec<f64>,
}

fn points(series: &[&VecDeque<f64>]) -> Vec<Point> {
  let len = series.iter().map(|s| s.len()).max().unwrap_or_default();
  let at = |values: &VecDeque<f64>, i: usize| {
    let offset = len - values.len();
    values
      .get(i.saturating_sub(offset))
      .copied()
      .unwrap_or_default()
  };
  (0..len)
    .map(|i| Point {
      x: i.to_string().into(),
      values: series.iter().map(|values| at(values, i)).collect(),
    })
    .collect()
}

fn gradient(color: Hsla) -> Background {
  linear_gradient(
    0.,
    linear_color_stop(color.opacity(0.4), 1.),
    linear_color_stop(color.opacity(0.), 0.),
  )
}

#[derive(Clone, Copy)]
enum Unit {
  Percent,
  Celsius,
  Rate,
}

impl Unit {
  fn format(self, value: f64) -> String {
    match self {
      Unit::Percent => format!("{value:.0}%"),
      Unit::Celsius => format!("{value:.0}°C"),
      Unit::Rate => rate(value),
    }
  }
}

struct Series {
  name: &'static str,
  color: Hsla,
  unit: Unit,
}

fn graph(
  id: &'static str,
  title: &'static str,
  points: Vec<Point>,
  series: &[Series],
  percent: bool,
) -> impl IntoElement {
  let units: Vec<Unit> = series.iter().map(|s| s.unit).collect();
  let mut chart = AreaChart::new(points)
    .id(id)
    .x(|p: &Point| p.x.clone())
    .x_axis(false)
    .grid(false)
    .y_axis(false)
    .tooltip_title(move |_| title.into())
    .tooltip_value(move |_, i, value| units[i].format(value).into());
  for (i, s) in series.iter().enumerate() {
    chart = chart
      .y(move |p: &Point| p.values[i])
      .stroke(s.color)
      .fill(gradient(s.color))
      .name(s.name)
      .natural();
  }
  if percent {
    chart = chart.y_domain(0., 100.);
  }
  div().flex_1().min_h(px(MIN_GRAPH)).w_full().child(chart)
}

fn stat(color: Hsla, icon: IconName, text: String) -> impl IntoElement {
  div()
    .flex()
    .gap_1()
    .items_center()
    .text_xs()
    .text_color(color)
    .child(Icon::new(icon).small())
    .child(text)
}

fn as_f64(values: &VecDeque<f32>) -> VecDeque<f64> {
  values.iter().map(|v| *v as f64).collect()
}

impl SysinfoPanel {
  pub(super) fn cpu(&self, theme: &Theme, cx: &Context<'_, Self>) -> impl IntoElement {
    let monitor = cx.system_monitor();
    let history = monitor.history(cx);
    let sample = monitor.sample(cx);
    let (usage, temperature) = (theme.chart_1, theme.danger);

    card(cx)
      .child(div().text_sm().font_bold().child("CPU"))
      .child(graph(
        "system-cpu",
        "CPU",
        points(&[&as_f64(&history.cpu), &as_f64(&history.cpu_temperature)]),
        &[
          Series {
            name: "Usage",
            color: usage,
            unit: Unit::Percent,
          },
          Series {
            name: "Temperature",
            color: temperature,
            unit: Unit::Celsius,
          },
        ],
        true,
      ))
      .child(
        div()
          .flex()
          .gap_3()
          .justify_center()
          .when_some(sample, |d, s| {
            d.child(stat(usage, IconName::Gauge, format!("{:.0}%", s.cpu)))
              .when_some(s.cpu_temperature, |d, t| {
                d.child(stat(temperature, IconName::Flame, format!("{t:.0}°C")))
              })
          }),
      )
  }

  pub(super) fn memory(&self, theme: &Theme, cx: &Context<'_, Self>) -> impl IntoElement {
    let monitor = cx.system_monitor();
    let memory = as_f64(&monitor.history(cx).memory);
    let color = theme.chart_2;

    card(cx)
      .child(div().text_sm().font_bold().child("Memory"))
      .child(graph(
        "system-memory",
        "Memory",
        points(&[&memory]),
        &[Series {
          name: "Used",
          color,
          unit: Unit::Percent,
        }],
        true,
      ))
      .child(
        div()
          .flex()
          .justify_center()
          .when_some(monitor.sample(cx), |d, s| {
            let percent = s.memory_used as f64 / s.memory_total.max(1) as f64 * 100.;
            d.child(stat(
              color,
              IconName::MemoryStick,
              format!("{:.1} GiB · {percent:.0}%", gib(s.memory_used)),
            ))
          }),
      )
  }

  pub(super) fn network(&self, theme: &Theme, cx: &Context<'_, Self>) -> impl IntoElement {
    let monitor = cx.system_monitor();
    let history = monitor.history(cx);
    let (down, up) = (theme.chart_1, theme.chart_2);

    card(cx)
      .child(div().text_sm().font_bold().child("Network"))
      .child(graph(
        "system-network",
        "Network",
        points(&[&history.network_rx, &history.network_tx]),
        &[
          Series {
            name: "Download",
            color: down,
            unit: Unit::Rate,
          },
          Series {
            name: "Upload",
            color: up,
            unit: Unit::Rate,
          },
        ],
        false,
      ))
      .child(
        div()
          .flex()
          .gap_3()
          .justify_center()
          .when_some(monitor.sample(cx), |d, s| {
            d.child(stat(down, IconName::Download, rate(s.network_rx)))
              .child(stat(up, IconName::Upload, rate(s.network_tx)))
          }),
      )
  }

  pub(super) fn gpu(&self, theme: &Theme, cx: &Context<'_, Self>) -> Option<impl IntoElement> {
    let monitor = cx.system_monitor();
    monitor
      .info(cx)?
      .gpus
      .iter()
      .any(|gpu| gpu.measurable)
      .then_some(())?;
    let history = monitor.history(cx);
    let (usage, memory, temperature) = (theme.chart_1, theme.chart_2, theme.danger);
    let measured = monitor.sample(cx).and_then(|s| s.gpus.first());

    Some(
      card(cx)
        .child(div().text_sm().font_bold().child("GPU"))
        .child(graph(
          "system-gpu",
          "GPU",
          points(&[
            &as_f64(&history.gpu),
            &as_f64(&history.gpu_memory),
            &as_f64(&history.gpu_temperature),
          ]),
          &[
            Series {
              name: "Usage",
              color: usage,
              unit: Unit::Percent,
            },
            Series {
              name: "VRAM",
              color: memory,
              unit: Unit::Percent,
            },
            Series {
              name: "Temperature",
              color: temperature,
              unit: Unit::Celsius,
            },
          ],
          true,
        ))
        .child(
          div()
            .flex()
            .gap_3()
            .justify_center()
            .when_some(measured, |d, gpu| {
              d.when_some(gpu.usage, |d, u| {
                d.child(stat(usage, IconName::Gauge, format!("{u:.0}%")))
              })
              .when_some(gpu.vram_used, |d, used| {
                d.child(stat(
                  memory,
                  IconName::MemoryStick,
                  format!("{:.1} GiB", gib(used)),
                ))
              })
              .when_some(gpu.temperature, |d, t| {
                d.child(stat(
                  temperature,
                  IconName::Thermometer,
                  format!("{t:.0}°C"),
                ))
              })
            }),
        ),
    )
  }
}

fn gib(bytes: u64) -> f64 {
  bytes as f64 / (1u64 << 30) as f64
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn points() {
    let cpu = VecDeque::from([10., 20., 30.]);
    let temperature = VecDeque::from([50., 60.]);
    let values: Vec<Vec<f64>> = super::points(&[&cpu, &temperature])
      .into_iter()
      .map(|p| p.values)
      .collect();
    // the shorter series lines up with the end
    assert_eq!(values, [[10., 50.], [20., 50.], [30., 60.]]);
  }
}
