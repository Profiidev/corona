use corona_components::{components::card::CardExt, i18n::format_time};
use corona_config::ConfigProvider;
use corona_surface::{
  control_center::{ControlCenter, ControlCenterPage},
  dashboard::DashboardCard,
};
use gpui_kit::{
  App, ClickEvent, Context, Div, InteractiveElement, IntoElement, ParentElement, Render,
  SharedString, Stateful, StatefulInteractiveElement, Styled, Subscription, Window,
  base::StyledExt,
  component::{ActiveTheme, Icon, Sizable},
  div,
  prelude::FluentBuilder,
};
use jiff::Zoned;
use rust_i18n::t;

use crate::{
  WeatherExt,
  ui::{
    calendar::CalendarPanel,
    page::{WeatherPanel, describe, icon},
  },
};

fn open<P: ControlCenterPage>(_: &ClickEvent, window: &mut Window, cx: &mut App) {
  ControlCenter::navigate_id(P::ID, window, cx)
}

fn muted(text: impl Into<String>, cx: &App) -> Div {
  div()
    .text_sm()
    .truncate()
    .text_color(cx.theme().muted_foreground)
    .child(text.into())
}

fn clock(cx: &App) -> Stateful<Div> {
  let theme = cx.theme();
  let now = Zoned::now();
  let config = &cx.config().control_center;
  let weather = cx.weather().current(cx).map(|w| {
    let current = &w.current;
    let condition = current.condition();
    (
      icon(condition, current.is_day),
      format!("{:.0}° · {}", current.temperature, describe(condition)),
    )
  });

  div()
    .id("dashboard-clock")
    .flex()
    .p_2()
    .gap_2()
    .card(cx)
    .flex_1()
    .min_w_0()
    .flex_col()
    .justify_center()
    .gap_0()
    .cursor_pointer()
    .on_click(open::<CalendarPanel>)
    .child(
      div()
        .text_2xl()
        .font_bold()
        .text_color(theme.colors.primary)
        .child(format_time(&config.time_format, &now)),
    )
    .child(muted(format_time(&config.date_format, &now), cx))
    .when_some(weather, |d, (icon, text)| {
      d.child(
        div()
          .id("dashboard-weather")
          .flex()
          .items_center()
          .gap_1()
          .text_sm()
          .text_color(theme.muted_foreground)
          .child(Icon::new(icon).small())
          .child(div().truncate().child(text))
          .on_click(|e, window, cx| {
            cx.stop_propagation();
            open::<WeatherPanel>(e, window, cx)
          }),
      )
    })
}

/// The time, date and current weather
pub struct ClockCard {
  _subscription: Subscription,
}

impl DashboardCard for ClockCard {
  const ID: &'static str = "clock";

  fn title() -> SharedString {
    t!("app.dashboard.cards.clock").into()
  }

  fn init(_: &mut Window, cx: &mut Context<Self>) -> Self {
    let weather = cx.weather().weather.clone();
    Self {
      _subscription: cx.observe(&weather, |_, _, cx| cx.notify()),
    }
  }
}

impl Render for ClockCard {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    clock(cx)
  }
}
