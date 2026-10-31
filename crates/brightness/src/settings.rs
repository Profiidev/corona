//! The system page's night light, brightness polling and ddcutil settings

use corona_components::settings_fields::{choice, number, switch, text};
use corona_config::NightlightMode;
use corona_settings::{SettingsExt, item};
use gpui_kit::{App, component::setting::SettingItem};
use rust_i18n::t;

pub(crate) fn register(cx: &mut App) {
  cx.settings_mut()
    .group("system", "nightlight", 10, || {
      t!("app.settings.system.groups.nightlight").into()
    })
    .group("system", "polling", 20, || {
      t!("app.settings.system.groups.polling").into()
    })
    .group("system", "applies_after_a_restart", 30, || {
      t!("app.settings.system.groups.applies_after_a_restart").into()
    })
    .group_items("system", "nightlight", 0, nightlight)
    .group_items("system", "polling", 10, polling)
    .group_items("system", "applies_after_a_restart", 0, restart);
}

fn nightlight(_: &App) -> Vec<SettingItem> {
  vec![
    item(
      t!("app.settings.system.nightlight.mode.title"),
      t!("app.settings.system.nightlight.mode.description"),
      choice(
        &[
          (
            NightlightMode::Manual,
            t!("app.settings.options.nightlight_manual"),
          ),
          (
            NightlightMode::Solar,
            t!("app.settings.options.nightlight_solar"),
          ),
          (
            NightlightMode::Schedule,
            t!("app.settings.options.nightlight_schedule"),
          ),
        ],
        |c| c.nightlight.mode,
        |c, v| c.nightlight.mode = v,
      ),
    ),
    item(
      t!("app.settings.system.nightlight.temperature.title"),
      t!("app.settings.system.nightlight.temperature.description"),
      number(
        "nightlight.temperature",
        (1000., 6500., 100.),
        |c| c.nightlight.temperature as f64,
        |c, v| c.nightlight.temperature = v as u32,
      ),
    ),
    item(
      t!("app.settings.system.nightlight.start.title"),
      t!("app.settings.system.nightlight.start.description"),
      text(
        "nightlight.start",
        |c| c.nightlight.start.clone(),
        |c, v| c.nightlight.start = v,
      ),
    ),
    item(
      t!("app.settings.system.nightlight.end.title"),
      t!("app.settings.system.nightlight.end.description"),
      text(
        "nightlight.end",
        |c| c.nightlight.end.clone(),
        |c, v| c.nightlight.end = v,
      ),
    ),
    item(
      t!("app.settings.system.nightlight.transition.title"),
      t!("app.settings.system.nightlight.transition.description"),
      number(
        "nightlight.transition_minutes",
        (0., 180., 1.),
        |c| c.nightlight.transition_minutes as f64,
        |c, v| c.nightlight.transition_minutes = v as u32,
      ),
    ),
  ]
}

fn polling(_: &App) -> Vec<SettingItem> {
  vec![item(
    t!("app.settings.system.brightness.title"),
    t!("app.settings.system.brightness.description"),
    number(
      "brightness.poll_seconds",
      (1., 60., 1.),
      |c| c.brightness.poll_seconds as f64,
      |c, v| c.brightness.poll_seconds = v as u64,
    ),
  )]
}

fn restart(_: &App) -> Vec<SettingItem> {
  vec![item(
    t!("app.settings.system.ddcutil.title"),
    t!("app.settings.system.ddcutil.description"),
    switch(
      |c| c.brightness.enable_ddcutil,
      |c, v| c.brightness.enable_ddcutil = v,
    ),
  )]
}
