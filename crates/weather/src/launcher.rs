//! The forecast as a launcher detail view

use corona_launcher::{Action, DetailView, Icon, Outcome, View, detail_row, kit::opener, registry};
use gpui_kit::{App, assets::IconName};
use rust_i18n::t;

use crate::{
  Condition, WeatherExt, compass,
  ui::page::{degrees, describe, place_name, speed},
};

fn weather(cx: &App) -> View {
  let service = cx.weather();
  let mut rows = Vec::new();
  let body = match service.current(cx) {
    Some(w) => {
      let c = &w.current;
      rows.push(detail_row(
        t!("app.launcher.info.feels_like"),
        degrees(c.apparent_temperature, w.units),
      ));
      rows.push(detail_row(
        t!("app.launcher.info.humidity"),
        format!("{:.0}%", c.humidity),
      ));
      rows.push(detail_row(
        t!("app.launcher.info.wind"),
        format!(
          "{} {}",
          speed(c.wind_speed, w.units),
          t!(format!("app.weather.compass.{}", compass(c.wind_direction)))
        ),
      ));
      rows.push(detail_row(
        t!("app.launcher.info.uv"),
        format!("{:.0}", c.uv_index),
      ));
      let mut body = format!(
        "# {} {}\n\n{}\n",
        degrees(c.temperature, w.units),
        describe(Condition::from_code(c.code)),
        place_name(&w.location)
      );
      for day in w.daily.iter().take(7) {
        body.push_str(&format!(
          "\n- **{}** {} {} / {}",
          day.date,
          describe(Condition::from_code(day.code)),
          degrees(day.max, w.units),
          degrees(day.min, w.units)
        ));
      }
      body
    }
    None => service
      .error(cx)
      .map(str::to_string)
      .unwrap_or_else(|| t!("app.launcher.info.no_weather").to_string()),
  };
  View::Detail(DetailView {
    title: t!("app.launcher.info.weather").into(),
    markdown: body.into(),
    metadata: rows,
    actions: vec![Action::sync(
      "refresh",
      t!("app.launcher.info.refresh"),
      |_, _, cx| {
        cx.weather().refresh();
        Ok(Outcome::KeepOpen)
      },
    )],
  })
}

pub(crate) fn register(cx: &mut App) {
  registry::register(
    opener(
      "weather",
      t!("app.launcher.info.weather"),
      Icon::Tile(IconName::CloudSun, 195.),
      &["forecast", "temperature"],
      weather,
    ),
    cx,
  );
}
