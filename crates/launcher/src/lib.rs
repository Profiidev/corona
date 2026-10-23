pub mod args;
pub mod command;
pub mod matching;
pub mod registry;
pub mod state;
pub mod view;

use gpui_kit::App;

rust_i18n::i18n!("../../assets/locales", fallback = "en");

pub use args::{ArgSpec, ArgValue, Args};
pub use command::{
  Accessory, Action, Clip, Command, CommandSource, Confirm, Detail, DetailBody, Hud, Icon, Meta,
  Outcome, Scope, Section, SimpleCommand, SourceKind, Toast, ToastKind, Tone,
};
pub use matching::{Match, Query, Tier};
pub use view::{
  CustomView, DetailView, Field, FieldKind, Filter, FormView, Layout, ListView, View,
};

pub fn init(cx: &mut App) {
  registry::init(cx);
}
