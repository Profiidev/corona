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
