//! The location page, with the forecast's units and refresh, and the clock's options form

use corona_components::settings_fields::{choice, non_empty, number, switch, text};
use corona_config::{Units, bar::BarConfig};
use corona_settings::{
  SettingsExt, SettingsPage, Slot, WidgetOptions,
  bar::{form, form_input, form_key, form_row, options, set_options},
  group, item, page,
};
use gpui_kit::{
  App, IntoElement, ParentElement, Window, assets::IconName, component::setting::SettingPage,
};
use rust_i18n::t;

use crate::ui::clock::{Clock, Options};

pub struct LocationSettings;

impl SettingsPage for LocationSettings {
  const ID: &'static str = "location";
  const ORDER: i32 = 130;

  fn page(_: &App) -> SettingPage {
    location()
  }
}

pub(crate) fn register(cx: &mut App) {
  cx.settings_mut()
    .register::<LocationSettings>()
    .widget_options::<Clock>(WidgetOptions {
      form: clock_form,
      label: None,
    });
}

fn location() -> SettingPage {
  page(
    t!("app.settings.location.title"),
    IconName::CloudSun,
    vec![
      group(
        t!("app.settings.location.groups.location"),
        vec![
          item(
            t!("app.settings.location.city.title"),
            t!("app.settings.location.city.description"),
            text(
              "location.city",
              |c| c.location.city.clone().unwrap_or_default(),
              |c, v| c.location.city = non_empty(v),
            ),
          ),
          item(
            t!("app.settings.location.coordinates.title"),
            t!("app.settings.location.coordinates.description"),
            text(
              "location.coordinates",
              |c| match (c.location.latitude, c.location.longitude) {
                (Some(lat), Some(lon)) => format!("{lat}, {lon}"),
                _ => String::new(),
              },
              |c, v| {
                let parsed = v.split_once(',').and_then(|(lat, lon)| {
                  Some((lat.trim().parse().ok()?, lon.trim().parse().ok()?))
                });
                (c.location.latitude, c.location.longitude) = match parsed {
                  Some((lat, lon)) => (Some(lat), Some(lon)),
                  None => (None, None),
                };
              },
            ),
          ),
          item(
            t!("app.settings.location.locate_automatically.title"),
            t!("app.settings.location.locate_automatically.description"),
            switch(
              |c| c.location.auto_locate,
              |c, v| c.location.auto_locate = v,
            ),
          ),
        ],
      ),
      group(
        t!("app.settings.location.groups.weather"),
        vec![
          item(
            t!("app.settings.location.units.title"),
            t!("app.settings.location.units.description"),
            choice(
              &[
                (Units::Metric, t!("app.settings.options.metric")),
                (Units::Imperial, t!("app.settings.options.imperial")),
              ],
              |c| c.weather.units,
              |c, v| c.weather.units = v,
            ),
          ),
          item(
            t!("app.settings.location.refresh_every.title"),
            t!("app.settings.location.refresh_every.description"),
            number(
              "weather.refresh_minutes",
              (5., 240., 5.),
              |c| c.weather.refresh_minutes as f64,
              |c, v| c.weather.refresh_minutes = v as u64,
            ),
          ),
        ],
      ),
    ],
  )
}

fn clock_form(
  name: &str,
  bar: &BarConfig,
  slot: Slot,
  window: &mut Window,
  cx: &mut App,
) -> gpui_kit::AnyElement {
  let options: Options = options(bar, slot);
  let name_ = name.to_string();
  form()
    .child(form_row(
      t!("app.settings.bar.form.format"),
      form_input(
        form_key(name, slot, "format"),
        options.format,
        window,
        cx,
        move |format, cx| {
          set_options(cx, &name_, slot, Options { format });
        },
      ),
    ))
    .into_any_element()
}
