use corona_weather::{Units, Weather, compass};
use gpui_kit::{
  IntoElement, ParentElement, Styled,
  assets::IconName,
  base::StyledExt,
  component::{Icon, Sizable, Theme},
  div, px,
};

use crate::control_center::weather::{WeatherPanel, card, icon};

fn degrees(value: f64) -> String {
  format!("{value:.0}°")
}

fn speed(value: f64, units: Units) -> String {
  match units {
    Units::Metric => format!("{value:.0} km/h"),
    Units::Imperial => format!("{value:.0} mph"),
  }
}

pub(super) fn clock(time: &str) -> &str {
  time.split_once('T').map_or(time, |(_, clock)| clock)
}

fn detail(theme: &Theme, icon: IconName, label: &'static str, value: String) -> impl IntoElement {
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
  pub(super) fn hero(&self, theme: &Theme, weather: &Weather) -> impl IntoElement {
    let current = &weather.current;
    let today = weather.daily.first();

    card(theme).child(
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
                .child(format!("{}C", degrees(current.temperature))),
            )
            .children(today.map(|day| {
              div()
                .text_sm()
                .text_color(theme.colors.primary)
                .child(format!("{:.0} / {}C", day.min, degrees(day.max)))
            }))
            .child(div().text_sm().child(current.condition().description()))
            .child(
              div()
                .text_xs()
                .text_color(theme.colors.muted_foreground)
                .truncate()
                .child(weather.location.name.clone()),
            ),
        ),
    )
  }

  pub(super) fn details(&self, theme: &Theme, weather: &Weather) -> impl IntoElement {
    let current = &weather.current;
    let mut details = card(theme).gap_1().flex_1().child(detail(
      theme,
      IconName::Feather,
      "Feels like",
      format!("{}C", degrees(current.apparent_temperature)),
    ));
    if let Some(today) = weather.daily.first() {
      details = details
        .child(detail(
          theme,
          IconName::ThermometerSnowflake,
          "Temperature min",
          format!("{}C", degrees(today.min)),
        ))
        .child(detail(
          theme,
          IconName::ThermometerSun,
          "Temperature max",
          format!("{}C", degrees(today.max)),
        ));
    }
    details = details
      .child(detail(
        theme,
        IconName::Wind,
        "Wind",
        format!(
          "{} {}",
          speed(current.wind_speed, weather.units),
          compass(current.wind_direction)
        ),
      ))
      .child(detail(
        theme,
        IconName::Droplets,
        "Humidity",
        format!("{:.0}%", current.humidity),
      ));
    if let Some(today) = weather.daily.first() {
      details = details
        .child(detail(
          theme,
          IconName::Sunrise,
          "Sunrise",
          clock(&today.sunrise).to_string(),
        ))
        .child(detail(
          theme,
          IconName::Sunset,
          "Sunset",
          clock(&today.sunset).to_string(),
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
        "Elevation",
        format!("{:.0} m", weather.elevation),
      ))
      .child(detail(
        theme,
        IconName::SunDim,
        "UV index",
        format!("{:.1}", current.uv_index),
      ))
      .child(detail(
        theme,
        IconName::Clock,
        "Timezone",
        format!("{} ({timezone})", weather.timezone_abbreviation),
      ))
  }
}

#[cfg(test)]
mod tests {
  #[test]
  fn clock() {
    assert_eq!(super::clock("2026-10-01T07:10"), "07:10");
    assert_eq!(super::clock("07:10"), "07:10");
  }
}
