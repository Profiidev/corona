use corona_weather::{Weather, weekday};
use gpui_kit::{
  Context, IntoElement, ParentElement, Styled,
  base::StyledExt,
  component::{
    Icon, Sizable, Theme,
    button::{Button, ButtonVariants},
    scroll::ScrollableElement,
  },
  div,
  prelude::FluentBuilder,
};

use crate::control_center::weather::{Tab, WeatherPanel, card, current::clock, icon};

impl WeatherPanel {
  pub(super) fn forecast(
    &self,
    theme: &Theme,
    weather: &Weather,
    cx: &Context<'_, Self>,
  ) -> impl IntoElement {
    let tabs = div()
      .flex()
      .gap_1()
      .children(
        [(Tab::Daily, "Daily"), (Tab::Hourly, "Hourly")].map(|(tab, label)| {
          Button::new(label)
            .label(label)
            .small()
            .flex_1()
            .cursor_pointer()
            .when(tab == self.tab, |b| b.primary())
            .on_click(cx.listener(move |this, _, _, cx| {
              this.tab = tab;
              cx.notify();
            }))
        }),
      );

    let row = || {
      div()
        .flex()
        .flex_col()
        .w_full()
        .p_2()
        .rounded_xl()
        .bg(theme.colors.background)
    };
    let muted = |text: String| {
      div()
        .text_xs()
        .text_color(theme.colors.muted_foreground)
        .truncate()
        .child(text)
    };
    let heading = |icon_name, title: String, value: String| {
      div()
        .flex()
        .gap_2()
        .items_center()
        .child(
          Icon::new(icon_name)
            .small()
            .text_color(theme.colors.primary),
        )
        .child(
          div()
            .flex_1()
            .min_w_0()
            .text_sm()
            .font_bold()
            .truncate()
            .child(title),
        )
        .child(div().text_xs().child(value))
    };

    let rows: Vec<_> = match self.tab {
      Tab::Daily => weather
        .daily
        .iter()
        .enumerate()
        .map(|(i, day)| {
          let name = match i {
            0 => "Today".to_string(),
            _ => weekday(&day.date).unwrap_or(&day.date).to_string(),
          };
          row()
            .child(heading(
              icon(day.condition(), true),
              name,
              format!("{:.0}° / {:.0}°", day.min, day.max),
            ))
            .child(muted(day.condition().description().into()))
        })
        .collect(),
      Tab::Hourly => weather
        .hourly
        .iter()
        .map(|hour| {
          row()
            .child(heading(
              icon(hour.condition(), hour.is_day),
              clock(&hour.time).to_string(),
              format!("{:.0}°", hour.temperature),
            ))
            .child(muted(format!(
              "{} · {:.0}% rain",
              hour.condition().description(),
              hour.precipitation_probability
            )))
        })
        .collect(),
    };

    card(theme).flex_1().min_h_0().child(tabs).child(
      div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .gap_1()
        .overflow_y_scrollbar()
        .id(match self.tab {
          Tab::Daily => "weather-daily",
          Tab::Hourly => "weather-hourly",
        })
        .children(rows),
    )
  }
}
