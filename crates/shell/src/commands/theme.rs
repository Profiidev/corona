use anyhow::Result;
use clap::ValueEnum;
use corona_components::assets::{apply_theme, theme_names, toggle_mode};
use corona_ipc::{IpcCommand, IpcServer};
use gpui_kit::{
  App,
  component::{ActiveTheme, Theme, ThemeMode},
};
use serde::{Deserialize, Serialize};

pub fn register_commands(server: &mut IpcServer) {
  server
    .register::<SetMode>()
    .register::<SetTheme>()
    .register::<ListThemes>();
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, ValueEnum)]
pub enum Mode {
  /// Switch to dark mode
  Dark,
  /// Switch to light mode
  Light,
  /// Switch between dark and light mode
  Toggle,
  /// Print the current mode
  Get,
}

pub struct SetMode;

impl IpcCommand for SetMode {
  const COMMAND: &'static str = "theme:mode";

  type Payload = Mode;
  /// `dark` or `light`, the mode afterwards
  type Response = String;

  fn handle(mode: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    match mode {
      Mode::Dark => Theme::change(ThemeMode::Dark, None, cx),
      Mode::Light => Theme::change(ThemeMode::Light, None, cx),
      Mode::Toggle => toggle_mode(cx),
      Mode::Get => {}
    }
    Ok(
      if cx.theme().is_dark() {
        "dark"
      } else {
        "light"
      }
      .to_string(),
    )
  }
}

pub struct SetTheme;

impl IpcCommand for SetTheme {
  const COMMAND: &'static str = "theme:set";

  type Payload = String;
  type Response = ();

  fn handle(name: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    apply_theme(&name, cx)
  }
}

pub struct ListThemes;

impl IpcCommand for ListThemes {
  const COMMAND: &'static str = "theme:list";

  type Payload = ();
  type Response = Vec<String>;

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    Ok(theme_names(cx))
  }
}
