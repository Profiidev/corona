//! The battery as a detail view, picking and cycling power profiles

use corona_components::time::span;
use std::rc::Rc;

use crate::{
  PowerExt,
  commands::{CycleProfile, SetProfile},
};
use corona_ipc::IpcCommand;
use corona_launcher::{
  Action, DetailView, Icon, Meta, Outcome, View,
  kit::{Choices, Group, opener, pick_or_list, watch},
  registry, simple,
};
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

pub(crate) fn register(cx: &mut App) {
  power_profile(cx);
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

fn power_profile(cx: &mut App) {
  let commands = vec![
    pick_or_list(
      "power_profile",
      t!("app.launcher.system.power_profile").into(),
      Icon::Tile(IconName::Gauge, 40.),
      &["performance", "battery", "balanced"],
      |cx| {
        cx.power()
          .profiles(cx)
          .map(|p| p.available.clone())
          .unwrap_or_default()
      },
      |name, cx| SetProfile::handle(name.to_string(), cx),
      profiles,
    ),
    simple(
      "power_profile:cycle",
      |cx| Meta {
        title: t!("app.launcher.system.cycle_profile").into(),
        subtitle: cx
          .power()
          .profiles(cx)
          .map(|p| SharedString::from(p.active.clone())),
        icon: Icon::Tile(IconName::Gauge, 40.),
        keywords: vec!["power profile".into()],
        ..Default::default()
      },
      |_| {
        vec![Action::sync(
          "cycle",
          t!("app.launcher.system.cycle_profile"),
          |_, _, cx| {
            CycleProfile::handle((), cx)?;
            Ok(Outcome::close())
          },
        )]
      },
    ),
  ];
  Group::new("system:power_profile", commands)
    .observe(|notify, cx| {
      let profiles = cx.power().profiles.clone();
      vec![watch(&profiles, &notify, cx)]
    })
    .register(cx);
}

fn profiles() -> corona_launcher::View {
  Choices {
    id: "power_profile",
    items: Rc::new(|cx| {
      cx.power()
        .profiles(cx)
        .map(|p| {
          p.available
            .iter()
            .map(|n| (n.clone().into(), n.clone().into()))
            .collect()
        })
        .unwrap_or_default()
    }),
    current: Rc::new(|cx| cx.power().profiles(cx).map(|p| p.active.clone().into())),
    apply: Rc::new(|name, cx| SetProfile::handle(name.to_string(), cx)),
    preview: None,
    icon: Icon::Named(IconName::Gauge),
  }
  .view(
    t!("app.launcher.system.power_profile"),
    t!("app.launcher.system.search_profiles"),
  )
}
