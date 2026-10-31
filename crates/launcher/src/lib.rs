pub mod args;
pub mod cli;
pub mod command;
pub mod commands;
pub mod common;
pub mod dmenu;
pub mod hooks;
pub mod ipc;
pub mod kit;
pub mod matching;
pub mod paste;
pub mod registry;
pub mod settings;
pub mod state;
pub mod ui;
pub mod view;

use corona_settings::SettingsExt;
use corona_surface::osd::OsdExt;
use gpui_kit::App;

rust_i18n::i18n!("../../assets/locales", fallback = "en");

pub use args::{ArgSpec, ArgValue, Args};
pub use command::{
  Accessory, Action, Clip, Command, CommandSource, Confirm, Detail, DetailBody, Evaluation, Hud,
  Icon, Meta, Outcome, Scope, Section, SimpleCommand, SourceKind, Toast, ToastKind, Tone, simple,
};
pub use matching::{Match, Query, Tier};
pub use view::{
  CustomView, DetailView, Field, FieldKind, Filter, FormView, Layout, ListView, View,
};

pub fn init(cx: &mut App) {
  registry::init(cx);
  ipc::register(cx);
  ui::bind_keys(cx);
  cx.osd_mut().register::<ui::hud::HudOsd>();
  cx.settings_mut().register::<settings::LauncherSettings>();
}
