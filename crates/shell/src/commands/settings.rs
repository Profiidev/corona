use anyhow::Result;
use clap::ValueEnum;
use corona_ipc::{IpcCommand, IpcServer};
use gpui_kit::App;
use serde::{Deserialize, Serialize};

use crate::settings;

pub fn register_commands(server: &mut IpcServer) {
  server.register::<SettingsWindow>();
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
      Action::Open => settings::open(page.as_deref(), cx),
      Action::Close => {
        settings::close(cx);
        Ok(())
      }
      Action::Toggle => settings::toggle(page.as_deref(), cx),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gpui_kit::{self as gpui, TestAppContext};

  #[gpui::test]
  fn open_toggle_close(cx: &mut TestAppContext) {
    cx.update(|cx| {
      gpui_kit::init(cx);
      cx.set_global(corona_config::Config::default());
    });
    let windows = |cx: &mut TestAppContext| cx.update(|cx| cx.windows().len());
    let run = |action, cx: &mut TestAppContext| {
      cx.update(|cx| SettingsWindow::handle((action, Some("idle".into())), cx))
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
  }
}
