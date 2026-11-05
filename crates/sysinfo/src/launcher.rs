//! System info as a launcher detail view

use std::{rc::Rc, time::SystemTime};

use corona_components::{
  format::{Base, bytes},
  time::span,
};
use corona_launcher::{
  Action, Command, DetailView, Icon, Meta, Outcome, View, detail_row, registry, simple,
};
use corona_utils::display::markdown_escape as escape;
use gpui_kit::{App, SharedString, assets::IconName};
use rust_i18n::t;

use crate::{SystemMonitorExt, sampler::unique_disks};

fn sysinfo(cx: &App) -> View {
  let monitor = cx.system_monitor();
  let mut rows = Vec::new();
  let mut body = String::new();
  if let Some(info) = monitor.info(cx) {
    body.push_str(&format!(
      "# {}\n\n{}\n",
      escape(&info.hostname),
      escape(&info.os)
    ));
    let uptime = SystemTime::now()
      .duration_since(info.booted)
      .unwrap_or_default();
    rows.push(detail_row(t!("app.launcher.info.uptime"), span(uptime)));
    rows.push(detail_row(
      t!("app.launcher.info.kernel"),
      info.kernel.clone(),
    ));
    rows.push(detail_row(
      t!("app.launcher.info.cpu"),
      format!("{} ({})", info.cpu_model, info.cpu_cores),
    ));
    for gpu in &info.gpus {
      rows.push(detail_row(t!("app.launcher.info.gpu"), gpu.name.clone()));
    }
    if let Some(compositor) = &info.compositor {
      rows.push(detail_row(
        t!("app.launcher.info.compositor"),
        compositor.clone(),
      ));
    }
  }
  if let Some(sample) = monitor.sample(cx) {
    rows.push(detail_row(
      t!("app.launcher.info.memory"),
      format!(
        "{} / {}",
        bytes(sample.memory_used, Base::Binary),
        bytes(sample.memory_total, Base::Binary)
      ),
    ));
    rows.push(detail_row(
      t!("app.launcher.info.cpu_usage"),
      format!("{:.0}%", sample.cpu),
    ));
    for disk in &unique_disks(sample.disks.clone()) {
      body.push_str(&format!(
        "\n- **{}** {} {} / {}",
        escape(&disk.mount_point),
        t!("app.launcher.info.free"),
        bytes(disk.available, Base::Binary),
        bytes(disk.total, Base::Binary)
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

pub(crate) fn register(cx: &mut App) {
  registry::register(sysinfo_command(), cx);
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
      vec![Action::sync("open", t!("app.launcher.open"), |_, _, cx| {
        Ok(Outcome::Push(sysinfo(cx)))
      })]
    },
  )
}
