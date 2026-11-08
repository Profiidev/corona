use corona_weather::{Units, Weather, compass};
use gpui_kit::{
  App, IntoElement, ParentElement, Styled,
  assets::IconName,
  base::StyledExt,
  component::ActiveTheme,
  component::{Icon, Sizable, Theme},
  div, px,
};

use crate::panel::weather::{WeatherPanel, card, describe, icon, place_name};
use corona_components::i18n::decimal;
use rust_i18n::t;

/// Rounds to a whole number, `+ 0.` turns -0 into 0 so (-0.5, 0) doesn't render as "-0"
pub(crate) fn whole(value: f64) -> f64 {
  value.round() + 0.
}

pub(crate) fn degrees(value: f64, units: Units) -> String {
  let unit = match units {
    Units::Metric => "C",
    Units::Imperial => "F",
  };
  format!("{:.0}°{unit}", whole(value))
}

pub(crate) fn speed(value: f64, units: Units) -> String {
  match units {
    Units::Metric => t!("app.weather.unit.kmh", value = format!("{value:.0}")).to_string(),
    Units::Imperial => t!("app.weather.unit.mph", value = format!("{value:.0}")).to_string(),
  }
}

pub(super) fn clock(time: &str) -> &str {
  time.split_once('T').map_or(time, |(_, clock)| clock)
}

fn detail(
  theme: &Theme,
  icon: IconName,
  label: impl IntoElement,
  value: String,
) -> impl IntoElement {
  div()
    .flex()
    .gap_2()
    .items_center()
    .text_xs()
    .child(Icon::new(icon).xsmall())
    .child(
      div()
        .flex_1()
        .min_w_0()
        .truncate()
        .text_color(theme.colors.muted_foreground)
        .child(label),
    )
    .child(div().font_bold().child(value))
}

impl WeatherPanel {
  pub(super) fn hero(&self, cx: &App, weather: &Weather) -> impl IntoElement {
    let theme = cx.theme();
    let current = &weather.current;
    let today = weather.daily.first();

    card(cx).child(
      div()
        .flex()
        .gap_4()
        .items_center()
        .justify_center()
        .py_4()
        .child(
          Icon::new(icon(current.condition(), current.is_day))
            .size(px(64.))
            .text_color(theme.chart_2),
        )
        .child(
          div()
            .flex()
            .flex_col()
            .min_w_0()
            .child(
              div()
                .text_3xl()
                .font_bold()
                .child(degrees(current.temperature, weather.units)),
            )
            .children(today.map(|day| {
              div()
                .text_sm()
                .text_color(theme.colors.primary)
                .child(format!(
                  "{} / {}",
                  degrees(day.min, weather.units),
                  degrees(day.max, weather.units)
                ))
            }))
            .child(div().text_sm().child(describe(current.condition())))
            .child(
              div()
                .text_xs()
                .text_color(theme.colors.muted_foreground)
                .truncate()
                .child(place_name(&weather.location)),
            ),
        ),
    )
  }

  pub(super) fn details(&self, cx: &App, weather: &Weather) -> impl IntoElement {
    let theme = cx.theme();
    let current = &weather.current;
    let mut details = card(cx).gap_1().flex_1().child(detail(
      theme,
      IconName::Feather,
      t!("app.weather.feels_like"),
      degrees(current.apparent_temperature, weather.units),
    ));
    let today = weather.daily.first();
    if let Some(today) = today {
      details = details
        .child(detail(
          theme,
          IconName::ThermometerSnowflake,
          t!("app.weather.temperature_min"),
          degrees(today.min, weather.units),
        ))
        .child(detail(
          theme,
          IconName::ThermometerSun,
          t!("app.weather.temperature_max"),
          degrees(today.max, weather.units),
        ));
    }
    details = details
      .child(detail(
        theme,
        IconName::Wind,
        t!("app.weather.wind"),
        format!(
          "{} {}",
          speed(current.wind_speed, weather.units),
          t!(format!(
            "app.weather.compass.{}",
            compass(current.wind_direction)
          ))
        ),
      ))
      .child(detail(
        theme,
        IconName::Droplets,
        t!("app.weather.humidity"),
        format!("{:.0}%", current.humidity),
      ));
    if let Some(sunrise) = today.and_then(|d| d.sunrise.as_deref()) {
      details = details.child(detail(
        theme,
        IconName::Sunrise,
        t!("app.weather.sunrise"),
        clock(sunrise).to_string(),
      ));
    }
    if let Some(sunset) = today.and_then(|d| d.sunset.as_deref()) {
      details = details.child(detail(
        theme,
        IconName::Sunset,
        t!("app.weather.sunset"),
        clock(sunset).to_string(),
      ));
    }
    let timezone = weather
      .timezone
      .rsplit_once('/')
      .map_or(weather.timezone.as_str(), |(_, city)| city)
      .replace('_', " ");
    details
      .child(detail(
        theme,
        IconName::Mountain,
        t!("app.weather.elevation"),
        format!("{:.0} m", weather.elevation),
      ))
      .child(detail(
        theme,
        IconName::SunDim,
        t!("app.weather.uv_index"),
        decimal(current.uv_index, 1),
      ))
      .child(detail(
        theme,
        IconName::Clock,
        t!("app.weather.timezone"),
        format!("{} ({timezone})", weather.timezone_abbreviation),
      ))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn degrees() {
    let m = Units::Metric;
    assert_eq!(super::degrees(21.4, m), "21°C");
    assert_eq!(super::degrees(21.6, m), "22°C");
    assert_eq!(super::degrees(-3.6, m), "-4°C");
    assert_eq!(super::degrees(0.0, m), "0°C");
    assert_eq!(super::degrees(71.6, Units::Imperial), "72°F");
  }

  #[test]
  fn degrees_negative_zero() {
    assert_eq!(super::degrees(-0.4, Units::Metric), "0°C");
  }

  #[test]
  fn speed() {
    assert_eq!(super::speed(12.6, Units::Metric), "13 km/h");
    assert_eq!(super::speed(12.4, Units::Imperial), "12 mph");
    assert_ne!(
      super::speed(10., Units::Metric),
      super::speed(10., Units::Imperial)
    );
  }

  #[test]
  fn clock() {
    assert_eq!(super::clock("2026-10-01T07:10"), "07:10");
    assert_eq!(super::clock("07:10"), "07:10");
    assert_eq!(super::clock(""), "");
    // only the first `T` splits
    assert_eq!(super::clock("aTbTc"), "bTc");
  }
}
