//! System info, battery and weather as detail views

use std::time::{Duration, SystemTime};

use corona_launcher::{Action, DetailView, Icon, Meta, Outcome, View, registry, simple};
use corona_power::PowerExt;
use corona_sysinfo::SystemMonitorExt;
use corona_weather::{Condition, WeatherExt, compass};
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

fn duration(d: Duration) -> String {
  let minutes = d.as_secs() / 60;
  let (days, hours, minutes) = (minutes / 1440, minutes / 60 % 24, minutes % 60);
  match (days, hours) {
    (0, 0) => format!("{minutes}m"),
    (0, _) => format!("{hours}h {minutes}m"),
    _ => format!("{days}d {hours}h {minutes}m"),
  }
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
    rows.push(row(t!("app.launcher.info.uptime"), duration(uptime)));
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
        rows.push(row(t!("app.launcher.info.to_empty"), duration(t)));
      }
      if let Some(t) = b.time_to_full.filter(|t| !t.is_zero()) {
        rows.push(row(t!("app.launcher.info.to_full"), duration(t)));
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

fn condition(code: u8) -> String {
  let key = format!("app.weather.condition.{}", Condition::from_code(code).key());
  t!(&key).to_string()
}

fn weather(cx: &App) -> View {
  let service = cx.weather();
  let mut rows: Rows = Vec::new();
  let body = match service.current(cx) {
    Some(w) => {
      let c = &w.current;
      rows.push(row(
        t!("app.launcher.info.feels_like"),
        format!("{:.0}°", c.apparent_temperature),
      ));
      rows.push(row(
        t!("app.launcher.info.humidity"),
        format!("{:.0}%", c.humidity),
      ));
      rows.push(row(
        t!("app.launcher.info.wind"),
        format!("{:.0} {}", c.wind_speed, compass(c.wind_direction)),
      ));
      rows.push(row(
        t!("app.launcher.info.uv"),
        format!("{:.0}", c.uv_index),
      ));
      let mut body = format!(
        "# {:.0}° {}\n\n{}\n",
        c.temperature,
        condition(c.code),
        w.location.name
      );
      for day in w.daily.iter().take(7) {
        body.push_str(&format!(
          "\n- **{}** {} {:.0}° / {:.0}°",
          day.date,
          condition(day.code),
          day.max,
          day.min
        ));
      }
      body
    }
    None => service
      .error(cx)
      .map(str::to_string)
      .unwrap_or_else(|| t!("app.launcher.info.no_weather").to_string()),
  };
  View::Detail(DetailView {
    title: t!("app.launcher.info.weather").into(),
    markdown: body.into(),
    metadata: rows,
    actions: vec![Action::sync(
      "refresh",
      t!("app.launcher.info.refresh"),
      |_, _, cx| {
        cx.weather().refresh();
        Ok(Outcome::KeepOpen)
      },
    )],
  })
}

pub fn init(cx: &mut App) {
  let detail = |id: &'static str,
                title: SharedString,
                icon: IconName,
                hue: f32,
                keywords: &'static [&'static str],
                view: fn(&App) -> View| {
    simple(
      id,
      move |_| Meta {
        title: title.clone(),
        icon: Icon::Tile(icon, hue),
        keywords: keywords.iter().map(|k| SharedString::from(*k)).collect(),
        ..Default::default()
      },
      move |_| {
        vec![Action::spawn(
          "open",
          t!("app.launcher.open"),
          move |_, cx| {
            // take a sample first when nothing samples yet
            let monitor = cx.system_monitor().clone();
            let idle = id == "sysinfo" && monitor.interval(cx).is_none();
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
                Ok(Outcome::Push(view(cx)))
              })
            }
          },
        )]
      },
    )
  };
  registry::register_all(
    [
      detail(
        "sysinfo",
        t!("app.launcher.info.system").into(),
        IconName::Cpu,
        120.,
        &["system", "uptime", "kernel", "memory", "disk"],
        sysinfo,
      ),
      detail(
        "battery",
        t!("app.launcher.info.battery").into(),
        IconName::BatteryFull,
        100.,
        &["power", "charge"],
        battery,
      ),
      detail(
        "weather",
        t!("app.launcher.info.weather").into(),
        IconName::CloudSun,
        195.,
        &["forecast", "temperature"],
        weather,
      ),
    ],
    cx,
  );
}
