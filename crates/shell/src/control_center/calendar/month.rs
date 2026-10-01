use gpui_kit::{
  Context, IntoElement, ParentElement, Styled,
  assets::IconName,
  base::StyledExt,
  component::{Sizable, Theme, button::Button},
  div,
  prelude::FluentBuilder,
  px,
};
use jiff::{ToSpan, civil::Date};

use crate::control_center::calendar::{CalendarPanel, today};

const WEEKDAYS: [&str; 7] = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"];

fn grid(first: Date) -> Vec<Date> {
  let back = i64::from(first.weekday().to_monday_zero_offset());
  let start = first.checked_sub(back.days()).unwrap_or(first);
  start.series(1.day()).take(42).collect()
}

impl CalendarPanel {
  fn shift(&mut self, months: i32, cx: &mut Context<Self>) {
    if let Ok(shown) = self.shown.checked_add(months.months()) {
      self.shown = shown;
      cx.notify();
    }
  }

  pub(super) fn month(&self, theme: &Theme, cx: &Context<'_, Self>) -> impl IntoElement {
    let now = today();
    let days = grid(self.shown);

    let nav = |id: &'static str, icon: IconName, tooltip: &'static str| {
      Button::new(id)
        .icon(icon)
        .small()
        .tooltip(tooltip)
        .cursor_pointer()
    };

    div()
      .flex()
      .flex_col()
      .w_full()
      .gap_2()
      .p_2()
      .rounded_xl()
      .bg(theme.colors.accent)
      .border_color(theme.border)
      .border_1()
      .child(
        div()
          .flex()
          .gap_2()
          .items_center()
          .child(
            div()
              .text_sm()
              .font_bold()
              .child(self.shown.strftime("%B %Y").to_string().to_uppercase()),
          )
          .child(div().flex_1().h(px(1.)).bg(theme.border))
          .child(
            nav("calendar-previous", IconName::ChevronLeft, "Previous month")
              .on_click(cx.listener(|this, _, _, cx| this.shift(-1, cx))),
          )
          .child(
            nav("calendar-today", IconName::CalendarDays, "Today").on_click(cx.listener(
              |this, _, _, cx| {
                this.shown = today().first_of_month();
                cx.notify();
              },
            )),
          )
          .child(
            nav("calendar-next", IconName::ChevronRight, "Next month")
              .on_click(cx.listener(|this, _, _, cx| this.shift(1, cx))),
          ),
      )
      .child(div().flex().children(WEEKDAYS.map(|day| {
        div()
          .flex_1()
          .flex()
          .justify_center()
          .text_xs()
          .font_bold()
          .text_color(theme.colors.primary)
          .child(day)
      })))
      .children(days.chunks(7).map(|week| {
        div().flex().children(week.iter().map(|day| {
          let other_month = day.month() != self.shown.month();
          div().flex_1().flex().justify_center().child(
            div()
              .flex()
              .items_center()
              .justify_center()
              .size(px(28.))
              .rounded_full()
              .text_sm()
              .when(other_month, |d| d.text_color(theme.colors.muted_foreground))
              .when(*day == now, |d| {
                d.bg(theme.chart_2)
                  .text_color(theme.colors.primary_foreground)
                  .font_bold()
              })
              .child(day.day().to_string()),
          )
        }))
      }))
  }
}

#[cfg(test)]
mod tests {
  #[test]
  fn grid() {
    let days = super::grid(jiff::civil::date(2026, 9, 1));
    assert_eq!(days.len(), 42);
    // September 2026 starts on a Tuesday, the grid on Monday the 31st of August
    assert_eq!(days[0], jiff::civil::date(2026, 8, 31));
    assert_eq!(days[12], jiff::civil::date(2026, 9, 12));
    assert_eq!(days[41], jiff::civil::date(2026, 10, 11));
  }
}
