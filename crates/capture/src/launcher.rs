//! Screenshot and color picker as launcher commands

use std::rc::Rc;

use corona_ipc::IpcCommand;
use corona_launcher::{Action, Command, Icon, Meta, Outcome, registry, simple};
use gpui_kit::{App, assets::IconName};
use rust_i18n::t;

use crate::{
  commands::{ColorPicker, Screenshot},
  ui::screenshot::mode::Mode,
};

/// Screenshot and color picker
pub(crate) fn register(cx: &mut App) {
  let commands: Vec<Rc<dyn Command>> = vec![
    simple(
      "screenshot",
      |_| Meta {
        title: t!("app.launcher.tools.screenshot").into(),
        icon: Icon::Tile(IconName::Camera, 190.),
        keywords: vec!["capture".into(), "screen".into()],
        ..Default::default()
      },
      |_| {
        [
          (
            "selection",
            t!("app.launcher.tools.screenshot"),
            Mode::Selection,
          ),
          (
            "monitor",
            t!("app.launcher.tools.screenshot_monitor"),
            Mode::Monitor,
          ),
          (
            "window",
            t!("app.launcher.tools.screenshot_window"),
            Mode::Window,
          ),
        ]
        .into_iter()
        .map(|(id, title, mode)| {
          Action::sync(id, title, move |_, _, cx| {
            Screenshot::handle(mode, cx)?;
            Ok(Outcome::close())
          })
        })
        .collect()
      },
    ),
    simple(
      "colorpicker",
      |_| Meta {
        title: t!("app.launcher.tools.color_picker").into(),
        icon: Icon::Tile(IconName::Pipette, 320.),
        keywords: vec!["color".into(), "colour".into(), "eyedropper".into()],
        ..Default::default()
      },
      |_| {
        vec![Action::sync(
          "pick",
          t!("app.launcher.tools.color_picker"),
          |_, _, cx| {
            ColorPicker::handle((), cx)?;
            Ok(Outcome::close())
          },
        )]
      },
    ),
  ];
  registry::register_all(commands, cx);
}
