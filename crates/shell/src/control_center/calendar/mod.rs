use std::time::Duration;

use corona_weather::WeatherExt;
use gpui_kit::{
  App, Context, IntoElement, ParentElement, Render, Styled, Task, Window,
  base::{StyledExt, Transition, transition},
  component::{ActiveTheme, Theme},
  div, px,
};
use jiff::{Zoned, civil::Date};

use crate::control_center::{ControlCenterPanel, utils::ring, variants::ControlCenterType};

mod month;

const TICK: Duration = Duration::from_secs(1);
const RING: f32 = 44.;
const RING_WIDTH: f32 = 4.;

pub struct CalendarPanel {
  shown: Date,
  _ticker: Task<()>,
}

impl ControlCenterPanel for CalendarPanel {
  const TYPE: ControlCenterType = ControlCenterType::Calendar;
  const HEIGHT: f32 = 430.0;

  fn init(_window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
    let ticker = cx.spawn(async move |this, cx| {
      loop {
        cx.background_executor().timer(TICK).await;
        if this.update(cx, |_, cx| cx.notify()).is_err() {
          break;
        }
      }
    });
    Self {
      shown: today().first_of_month(),
      _ticker: ticker,
    }
  }
}

fn today() -> Date {
  Zoned::now().date()
}

fn gmt_offset(seconds: i32) -> String {
  let sign = if seconds < 0 { '-' } else { '+' };
  let (hours, minutes) = (seconds.abs() / 3600, seconds.abs() % 3600 / 60);
  match minutes {
    0 => format!("GMT{sign}{hours}"),
    _ => format!("GMT{sign}{hours}:{minutes:02}"),
  }
}

fn place(cx: &App) -> Option<String> {
  let name = &cx.weather().current(cx)?.location.name;
  Some(name.split(',').next().unwrap_or(name).trim().to_string())
}

impl CalendarPanel {
  fn header(&self, theme: &Theme, seconds: f32, cx: &App) -> impl IntoElement {
    let now = Zoned::now();
    let offset = gmt_offset(now.offset().seconds());

    div()
      .flex()
      .gap_3()
      .items_center()
      .w_full()
      .px_4()
      .py_2()
      .rounded_xl()
      .bg(theme.colors.accent)
      .border_color(theme.border)
      .border_1()
      .child(
        div()
          .text_size(px(48.))
          .line_height(px(52.))
          .font_bold()
          .child(now.day().to_string()),
      )
      .child(
        div()
          .flex()
          .flex_col()
          .flex_1()
          .min_w_0()
          .child(
            div()
              .flex()
              .gap_2()
              .items_baseline()
              .child(
                div()
                  .text_xl()
                  .font_bold()
                  .child(now.strftime("%B").to_string().to_uppercase()),
              )
              .child(
                div()
                  .text_sm()
                  .font_bold()
                  .text_color(theme.colors.muted_foreground)
                  .child(now.year().to_string()),
              ),
          )
          .child(
            div()
              .flex()
              .gap_1()
              .items_baseline()
              .text_sm()
              .truncate()
              .children(place(cx))
              .child(
                div()
                  .text_xs()
                  .text_color(theme.colors.muted_foreground)
                  .child(format!("({offset})")),
              ),
          ),
      )
      .child(
        div()
          .relative()
          .flex()
          .flex_col()
          .flex_none()
          .items_center()
          .justify_center()
          .size(px(RING))
          .text_xs()
          .line_height(px(11.))
          .font_bold()
          .child(ring(seconds, theme.colors.primary, RING_WIDTH))
          .child(format!("{:02}", now.hour()))
          .child(format!("{:02}", now.minute())),
      )
  }
}

impl Render for CalendarPanel {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let seconds = transition(
      "calendar-seconds",
      f32::from(Zoned::now().second()) / 60.,
      Transition::new(cx.theme().motion_tokens().duration_normal)
        .easing(cx.theme().motion_tokens().easing_move.clone()),
      window,
      cx,
    );
    let theme = cx.theme();

    div()
      .flex()
      .flex_col()
      .size_full()
      .gap_2()
      .child(self.header(theme, seconds, cx))
      .child(self.month(theme, cx))
  }
}

#[cfg(test)]
mod tests {
  #[test]
  fn gmt_offset() {
    assert_eq!(super::gmt_offset(2 * 3600), "GMT+2");
    assert_eq!(super::gmt_offset(-5 * 3600), "GMT-5");
    assert_eq!(super::gmt_offset(5 * 3600 + 30 * 60), "GMT+5:30");
    assert_eq!(super::gmt_offset(0), "GMT+0");
  }
}
