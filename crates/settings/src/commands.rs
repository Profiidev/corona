use anyhow::Result;
use clap::ValueEnum;
use corona_ipc::{IpcCommand, IpcExt};
use gpui_kit::App;
use serde::{Deserialize, Serialize};

use crate::SettingsExt;

pub fn register_commands(cx: &mut App) {
  cx.ipc_mut()
    .register::<SettingsWindow>()
    .register::<ListPages>();
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, ValueEnum)]
pub enum Action {
  /// Show the settings window
  Open,
  /// Close it
  Close,
  /// Open it, or close it when open
  Toggle,
}

pub struct SettingsWindow;

impl IpcCommand for SettingsWindow {
  const COMMAND: &'static str = "settings:window";

  /// what to do, and the page to open on
  type Payload = (Action, Option<String>);
  type Response = ();

  fn handle((action, page): Self::Payload, cx: &mut App) -> Result<Self::Response> {
    match action {
      Action::Open => crate::open(page.as_deref(), cx),
      Action::Close => {
        crate::close(cx);
        Ok(())
      }
      Action::Toggle => crate::toggle(page.as_deref(), cx),
    }
  }
}

/// The settings page ids, in sidebar order
pub struct ListPages;

impl IpcCommand for ListPages {
  const COMMAND: &'static str = "settings:list";

  type Payload = ();
  type Response = Vec<String>;

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    Ok(cx.settings().ids().into_iter().map(String::from).collect())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gpui_kit::{self as gpui, TestAppContext};

  #[test]
  fn command_names_are_stable() {
    // clients of other versions send these
    assert_eq!(SettingsWindow::COMMAND, "settings:window");
    assert_eq!(ListPages::COMMAND, "settings:list");
  }

  #[gpui::test]
  fn open_toggle_close(cx: &mut TestAppContext) {
    cx.update(|cx| {
      gpui_kit::init(cx);
      cx.set_global(corona_config::Config::default());
      corona_surface::bar::BarState::init(cx);
      crate::init(cx);
    });
    let windows = |cx: &mut TestAppContext| cx.update(|cx| cx.windows().len());
    let run = |action, cx: &mut TestAppContext| {
      cx.update(|cx| SettingsWindow::handle((action, Some("bar".into())), cx))
        .unwrap();
      cx.run_until_parked();
    };
    run(Action::Open, cx);
    assert_eq!(windows(cx), 1);
    run(Action::Open, cx);
    assert_eq!(windows(cx), 1);
    run(Action::Toggle, cx);
    assert_eq!(windows(cx), 0);
    run(Action::Toggle, cx);
    assert_eq!(windows(cx), 1);
    run(Action::Close, cx);
    assert_eq!(windows(cx), 0);
    run(Action::Close, cx);
    assert_eq!(windows(cx), 0);
    assert_eq!(
      cx.update(|cx| ListPages::handle((), cx)).unwrap(),
      [
        "appearance",
        "bar",
        "wallpaper",
        "osd",
        "control_center",
        "system"
      ]
    );
  }
}
