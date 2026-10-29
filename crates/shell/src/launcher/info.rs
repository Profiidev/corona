//! System info and battery as detail views

use std::{
  rc::Rc,
  time::{Duration, SystemTime},
};

use corona_components::time::span;

use corona_launcher::{
  Action, Command, DetailView, Icon, Meta, Outcome, View, kit::opener, registry, simple,
};
use corona_power::PowerExt;
use corona_sysinfo::SystemMonitorExt;
use gpui_kit::{App, SharedString, assets::IconName};
use rust_i18n::t;

fn bytes(n: u64) -> String {
  const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
  let mut value = n as f64;
  let mut unit = 0;
  while value >= 1024. && unit < UNITS.len() - 1 {
    value /= 1024.;
    unit += 1;
  }
  format!("{value:.1} {}", UNITS[unit])
}

type Rows = Vec<(SharedString, SharedString)>;

fn row(
  key: impl Into<SharedString>,
  value: impl Into<SharedString>,
) -> (SharedString, SharedString) {
  (key.into(), value.into())
}

fn sysinfo(cx: &App) -> View {
  let monitor = cx.system_monitor();
  let mut rows: Rows = Vec::new();
  let mut body = String::new();
  if let Some(info) = monitor.info(cx) {
    body.push_str(&format!("# {}\n\n{}\n", info.hostname, info.os));
    let uptime = SystemTime::now()
      .duration_since(info.booted)
      .unwrap_or_default();
    rows.push(row(t!("app.launcher.info.uptime"), span(uptime)));
    rows.push(row(t!("app.launcher.info.kernel"), info.kernel.clone()));
    rows.push(row(
      t!("app.launcher.info.cpu"),
      format!("{} ({})", info.cpu_model, info.cpu_cores),
    ));
    for gpu in &info.gpus {
      rows.push(row(t!("app.launcher.info.gpu"), gpu.name.clone()));
    }
    if let Some(compositor) = &info.compositor {
      rows.push(row(t!("app.launcher.info.compositor"), compositor.clone()));
    }
  }
  if let Some(sample) = monitor.sample(cx) {
    rows.push(row(
      t!("app.launcher.info.memory"),
      format!(
        "{} / {}",
        bytes(sample.memory_used),
        bytes(sample.memory_total)
      ),
    ));
    rows.push(row(
      t!("app.launcher.info.cpu_usage"),
      format!("{:.0}%", sample.cpu),
    ));
    for disk in &sample.disks {
      body.push_str(&format!(
        "\n- **{}** {} {} / {}",
        disk.mount_point,
        t!("app.launcher.info.free"),
        bytes(disk.available),
        bytes(disk.total)
      ));
    }
  }
  View::Detail(DetailView {
    title: t!("app.launcher.info.system").into(),
    markdown: body.into(),
    metadata: rows,
    actions: Vec::new(),
  })
}

fn battery(cx: &App) -> View {
  let mut rows: Rows = Vec::new();
  let body = match cx.power().battery(cx) {
    Some(b) => {
      rows.push(row(t!("app.launcher.info.state"), format!("{:?}", b.state)));
      if let Some(t) = b.time_to_empty.filter(|t| !t.is_zero()) {
        rows.push(row(t!("app.launcher.info.to_empty"), span(t)));
      }
      if let Some(t) = b.time_to_full.filter(|t| !t.is_zero()) {
        rows.push(row(t!("app.launcher.info.to_full"), span(t)));
      }
      rows.push(row(
        t!("app.launcher.info.health"),
        format!("{:.0}%", b.capacity),
      ));
      rows.push(row(
        t!("app.launcher.info.rate"),
        format!("{:.1} W", b.energy_rate),
      ));
      format!("# {:.0}%", b.percentage)
    }
    None => t!("app.launcher.info.no_battery").to_string(),
  };
  View::Detail(DetailView {
    title: t!("app.launcher.info.battery").into(),
    markdown: body.into(),
    metadata: rows,
    actions: Vec::new(),
  })
}

pub fn init(cx: &mut App) {
  registry::register(sysinfo_command(), cx);
  registry::register(
    opener(
      "battery",
      t!("app.launcher.info.battery"),
      Icon::Tile(IconName::BatteryFull, 100.),
      &["power", "charge"],
      battery,
    ),
    cx,
  );
}

fn sysinfo_command() -> Rc<dyn Command> {
  simple(
    "sysinfo",
    |_| Meta {
      title: t!("app.launcher.info.system").into(),
      icon: Icon::Tile(IconName::Cpu, 120.),
      keywords: ["system", "uptime", "kernel", "memory", "disk"]
        .map(SharedString::from)
        .into(),
      ..Default::default()
    },
    |_| {
      vec![Action::spawn("open", t!("app.launcher.open"), |_, cx| {
        // take a sample first when nothing samples yet
        let monitor = cx.system_monitor().clone();
        let idle = monitor.interval(cx).is_none();
        if idle {
          monitor.set_interval(Some(Duration::from_millis(500)), cx);
        }
        let cx = cx.to_async();
        async move {
          if idle {
            cx.background_executor()
              .timer(Duration::from_millis(700))
              .await;
          }
          cx.update(|cx| {
            if idle {
              monitor.set_interval(None, cx);
            }
            Ok(Outcome::Push(sysinfo(cx)))
          })
        }
      })]
    },
  )
}
