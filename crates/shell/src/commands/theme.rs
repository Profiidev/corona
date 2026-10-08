use anyhow::Result;
use clap::ValueEnum;
use corona_components::assets::{set_mode, set_theme, theme_names};
use corona_ipc::{IpcCommand, IpcServer};
use gpui_kit::{App, component::ActiveTheme};
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
    let dark = match mode {
      Mode::Dark => Some(true),
      Mode::Light => Some(false),
      Mode::Toggle => Some(!cx.theme().is_dark()),
      Mode::Get => None,
    };
    // the theme follows the settings only once this returns
    if let Some(dark) = dark {
      set_mode(dark, cx)?;
    }
    let dark = dark.unwrap_or_else(|| cx.theme().is_dark());
    Ok(if dark { "dark" } else { "light" }.to_string())
  }
}

pub struct SetTheme;

impl IpcCommand for SetTheme {
  const COMMAND: &'static str = "theme:set";

  type Payload = String;
  type Response = ();

  fn handle(name: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    set_theme(name, cx)
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

#[cfg(test)]
mod tests {
  use super::*;
  use crate::test_support::temp_config;
  use gpui_kit::{self as gpui, TestAppContext};

  #[gpui::test]
  fn modes(cx: &mut TestAppContext) {
    let _dir = temp_config();
    cx.update(|cx| {
      gpui_kit::init(cx);
      cx.set_global(corona_config::Config::default());
      corona_components::assets::load(cx).unwrap();
    });
    let mode = |m, cx: &mut TestAppContext| cx.update(|cx| SetMode::handle(m, cx)).unwrap();
    assert_eq!(mode(Mode::Dark, cx), "dark");
    cx.run_until_parked();
    assert_eq!(mode(Mode::Get, cx), "dark");
    assert_eq!(mode(Mode::Toggle, cx), "light");
    cx.run_until_parked();
    assert_eq!(mode(Mode::Get, cx), "light");
    assert_eq!(mode(Mode::Light, cx), "light");
  }

  #[gpui::test]
  fn unknown_theme_errors(cx: &mut TestAppContext) {
    cx.update(|cx| {
      gpui_kit::init(cx);
      cx.set_global(corona_config::Config::default());
      corona_components::assets::load(cx).unwrap();
      let names = ListThemes::handle((), cx).unwrap();
      assert!(!names.is_empty() && names.is_sorted());
      assert!(SetTheme::handle("No Such Theme".into(), cx).is_err());
    });
  }
}
