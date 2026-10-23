//! Monitors: enable, disable and mirror

use std::rc::Rc;

use corona_compositor::{CompositorExt, types::Monitor};
use corona_ipc::IpcCommand;
use corona_launcher::{
  Accessory, Action, Icon, ListView, Meta, Outcome, Tone,
  kit::{ListSource, opener, watch},
  registry, simple,
};
use gpui_kit::{App, assets::IconName};
use rust_i18n::t;

use crate::commands::monitor::{Change, Configure};

fn row(monitor: &Monitor, others: Vec<String>) -> Rc<dyn corona_launcher::Command> {
  let m = monitor.clone();
  simple(
    format!("monitor:{}", monitor.name),
    move |_| {
      let scale = m.scale.max(0.1);
      Meta {
        title: m.name.clone().into(),
        subtitle: Some(
          format!(
            "{}×{} @ {:.0} Hz, ×{}",
            m.width, m.height, m.refresh_rate, scale
          )
          .into(),
        ),
        icon: Icon::Named(IconName::Monitor),
        accessories: vec![match (m.disabled, m.mirror_of.as_str()) {
          (true, _) => Accessory::Tag(t!("app.launcher.monitors.disabled").into(), Tone::Off),
          (false, "none" | "") => {
            Accessory::Tag(t!("app.launcher.monitors.enabled").into(), Tone::Ok)
          }
          (false, source) => Accessory::Tag(
            t!("app.launcher.monitors.mirrors", source = source).into(),
            Tone::Warn,
          ),
        }],
        ..Default::default()
      }
    },
    {
      let m = monitor.clone();
      move |_| {
        let name = m.name.clone();
        let change = |change: Change, source: Option<String>, title| {
          let name = name.clone();
          Action::sync(
            format!("{change:?}").to_lowercase(),
            title,
            move |_, _, cx| {
              Configure::handle((name.clone(), change, source.clone()), cx)?;
              Ok(Outcome::KeepOpen)
            },
          )
          .keep_open()
        };
        let mut actions = vec![match m.disabled {
          true => change(Change::Enable, None, t!("app.launcher.monitors.enable")),
          false => change(Change::Disable, None, t!("app.launcher.monitors.disable")).destructive(),
        }];
        if !matches!(m.mirror_of.as_str(), "none" | "") {
          actions.push(change(
            Change::StopMirroring,
            None,
            t!("app.launcher.monitors.stop_mirroring"),
          ));
        }
        for other in &others {
          actions.push(change(
            Change::Mirror,
            Some(other.clone()),
            t!("app.launcher.monitors.mirror", source = other.clone()),
          ));
        }
        actions
      }
    },
  )
}

pub fn init(cx: &mut App) {
  registry::register(
    opener(
      "monitor:settings",
      t!("app.launcher.monitors.title"),
      Icon::Tile(IconName::MonitorCog, 250.),
      &["display", "screen", "resolution"],
      |_| {
        ListView::new(
          t!("app.launcher.monitors.title"),
          t!("app.launcher.monitors.search"),
          Rc::new(
            ListSource::new("monitors", |_, cx| {
              let monitors = cx.compositor().list_monitors(cx).to_vec();
              monitors
                .iter()
                .map(|m| {
                  let others = monitors
                    .iter()
                    .filter(|o| o.name != m.name)
                    .map(|o| o.name.clone())
                    .collect();
                  row(m, others)
                })
                .collect()
            })
            .observe(|notify, cx| {
              let monitors = cx.compositor().monitors.clone();
              vec![watch(&monitors, &notify, cx)]
            }),
          ),
        )
        .into_view()
      },
    ),
    cx,
  );
}
