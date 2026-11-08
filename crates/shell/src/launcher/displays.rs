//! Windows, monitors and turning the displays off as launcher commands

use corona_ipc::IpcCommand;
use corona_launcher::{Action, Icon, Meta, Outcome, kit::Group, simple};
use gpui_kit::{App, assets::IconName};
use rust_i18n::t;

use crate::ipc::windows::dpms::{Dpms, Power};

use super::{monitors, windows};
pub(crate) fn register(cx: &mut App) {
  windows::register(cx);
  windows::register_actions(cx);
  monitors::register(cx);
  dpms(cx);
}

fn dpms(cx: &mut App) {
  let off = simple(
    "dpms:off",
    |_| Meta {
      title: t!("app.launcher.system.displays_off").into(),
      icon: Icon::Tile(IconName::MonitorOff, 250.),
      keywords: vec!["dpms".into(), "screen".into(), "monitor".into()],
      ..Default::default()
    },
    |_| {
      vec![Action::sync(
        "off",
        t!("app.launcher.system.displays_off"),
        |_, _, cx| {
          Dpms::handle(Power::Off, cx)?;
          Ok(Outcome::close())
        },
      )]
    },
  );
  Group::new("system:dpms", vec![off]).register(cx);
}
