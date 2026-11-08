//! The location page, with the forecast's units and refresh, the clock's options form and
//! the control center's time and date formats

use corona_components::settings_fields::{choice, non_empty, number, switch, text};
use corona_config::{Units, Weekday, bar::BarConfig};
use corona_settings::{
  SettingsExt, Slot, WidgetOptions,
  bar::{form, form_input, form_key, form_row, options, set_options},
  group, item, page, settings_page,
};
use gpui_kit::{
  App, IntoElement, ParentElement, Window,
  assets::IconName,
  component::setting::{SettingItem, SettingPage},
};
use rust_i18n::t;

use crate::bar::clock::{Clock, Options};

settings_page!(LocationSettings, "location", 130, |cx| location());

pub(super) fn register(cx: &mut App) {
  cx.settings_mut()
    .register::<LocationSettings>()
    .group_items("control_center", "control_center", 0, clock_formats)
    .widget_options::<Clock>(WidgetOptions {
      form: clock_form,
      label: None,
    });
}

/// The control center clock's time and date formats and the calendar's first weekday
fn clock_formats(_: &App) -> Vec<SettingItem> {
  vec![
    item(
      t!("app.settings.control_center.time_format.title"),
      t!("app.settings.control_center.time_format.description"),
      text(
        "control_center.time_format",
        |c| c.control_center.time_format.clone(),
        |c, v| c.control_center.time_format = v,
      ),
    ),
    item(
      t!("app.settings.control_center.date_format.title"),
      t!("app.settings.control_center.date_format.description"),
      text(
        "control_center.date_format",
        |c| c.control_center.date_format.clone(),
        |c, v| c.control_center.date_format = v,
      ),
    ),
    item(
      t!("app.settings.control_center.week_starts_on.title"),
      t!("app.settings.control_center.week_starts_on.description"),
      choice(
        &[
          (Weekday::Monday, t!("app.settings.options.monday")),
          (Weekday::Sunday, t!("app.settings.options.sunday")),
        ],
        |c| c.control_center.week_start,
        |c, v| c.control_center.week_start = v,
      ),
    ),
  ]
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
                if v.trim().is_empty() {
                  (c.location.latitude, c.location.longitude) = (None, None);
                } else if let Some((lat, lon)) = parse_coordinates(&v) {
                  (c.location.latitude, c.location.longitude) = (Some(lat), Some(lon));
                }
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

/// Parses "lat, lon", rejecting non-finite and out-of-range values
fn parse_coordinates(v: &str) -> Option<(f64, f64)> {
  let (lat, lon) = v.split_once(',')?;
  let (lat, lon) = (
    lat.trim().parse::<f64>().ok()?,
    lon.trim().parse::<f64>().ok()?,
  );
  ((-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon)).then_some((lat, lon))
}

#[cfg(test)]
mod tests {
  use super::parse_coordinates;

  #[test]
  fn coordinates() {
    assert_eq!(parse_coordinates("47.5, 12.1"), Some((47.5, 12.1)));
    assert_eq!(parse_coordinates("-47.5,-12"), Some((-47.5, -12.0)));
    assert_eq!(parse_coordinates("47,5, 12,1"), None);
    assert_eq!(parse_coordinates("123, 10"), None);
    assert_eq!(parse_coordinates("10, 181"), None);
    assert_eq!(parse_coordinates("NaN, 10"), None);
    assert_eq!(parse_coordinates(""), None);
  }
}
