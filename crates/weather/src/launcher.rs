//! The forecast as a launcher detail view

use corona_launcher::{Action, DetailView, Icon, Outcome, View, detail_row, kit::opener, registry};
use gpui_kit::{App, assets::IconName};
use rust_i18n::t;

use crate::{Condition, WeatherExt, compass};

fn condition(code: u8) -> String {
  let key = format!("app.weather.condition.{}", Condition::from_code(code).key());
  t!(&key).to_string()
}

fn weather(cx: &App) -> View {
  let service = cx.weather();
  let mut rows = Vec::new();
  let body = match service.current(cx) {
    Some(w) => {
      let c = &w.current;
      rows.push(detail_row(
        t!("app.launcher.info.feels_like"),
        format!("{:.0}°", c.apparent_temperature),
      ));
      rows.push(detail_row(
        t!("app.launcher.info.humidity"),
        format!("{:.0}%", c.humidity),
      ));
      rows.push(detail_row(
        t!("app.launcher.info.wind"),
        format!("{:.0} {}", c.wind_speed, compass(c.wind_direction)),
      ));
      rows.push(detail_row(
        t!("app.launcher.info.uv"),
        format!("{:.0}", c.uv_index),
      ));
      let mut body = format!(
        "# {:.0}° {}\n\n{}\n",
        c.temperature,
        condition(c.code),
        w.location.name
      );
      for day in w.daily.iter().take(7) {
        body.push_str(&format!(
          "\n- **{}** {} {:.0}° / {:.0}°",
          day.date,
          condition(day.code),
          day.max,
          day.min
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
