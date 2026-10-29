//! Battery as a detail view

use corona_components::time::span;
use corona_launcher::{DetailView, Icon, View, kit::opener, registry};
use corona_power::PowerExt;
use gpui_kit::{App, SharedString, assets::IconName};
use rust_i18n::t;

fn row(
  key: impl Into<SharedString>,
  value: impl Into<SharedString>,
) -> (SharedString, SharedString) {
  (key.into(), value.into())
}

fn battery(cx: &App) -> View {
  let mut rows = Vec::new();
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
