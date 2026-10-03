use corona_weather::{Condition, WeatherExt};
use gpui_kit::{
  AnyElement, Context, Div, IntoElement, ParentElement, Render, Styled, Subscription, Window,
  assets::IconName,
  component::{ActiveTheme, Theme, button::Button, scroll::ScrollableElement},
  div,
  prelude::FluentBuilder,
};

use crate::control_center::{ControlCenterPanel, variants::ControlCenterType};

mod current;
mod forecast;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
  Daily,
  Hourly,
}

pub struct WeatherPanel {
  tab: Tab,
  _subscriptions: [Subscription; 2],
}

impl ControlCenterPanel for WeatherPanel {
  const TYPE: ControlCenterType = ControlCenterType::Weather;

  fn init(_window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
    let weather = cx.weather().clone();
    let subscriptions = [
      cx.observe(&weather.weather, |_, _, cx| cx.notify()),
      cx.observe(&weather.error, |_, _, cx| cx.notify()),
    ];
    Self {
      tab: Tab::Daily,
      _subscriptions: subscriptions,
    }
  }

  fn buttons(&mut self, _cx: &mut Context<Self>) -> Vec<AnyElement> {
    vec![
      Button::new("weather-refresh")
        .icon(IconName::RefreshCw)
        .tooltip("Refresh")
        .cursor_pointer()
        .on_click(|_, _, cx| cx.weather().refresh())
        .into_any_element(),
    ]
  }
}

fn card(theme: &Theme) -> Div {
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
}

fn icon(condition: Condition, is_day: bool) -> IconName {
  match condition {
    Condition::Clear if is_day => IconName::Sun,
    Condition::Clear => IconName::Moon,
    Condition::MainlyClear | Condition::PartlyCloudy if is_day => IconName::CloudSun,
    Condition::MainlyClear | Condition::PartlyCloudy => IconName::CloudMoon,
    Condition::Overcast | Condition::Unknown => IconName::Cloud,
    Condition::Fog => IconName::CloudFog,
    Condition::Drizzle | Condition::FreezingDrizzle => IconName::CloudDrizzle,
    Condition::Rain | Condition::FreezingRain => IconName::CloudRain,
    Condition::RainShowers => IconName::CloudRainWind,
    Condition::Snow | Condition::SnowGrains | Condition::SnowShowers => IconName::CloudSnow,
    Condition::Thunderstorm => IconName::CloudLightning,
    Condition::ThunderstormHail => IconName::CloudHail,
  }
}

impl Render for WeatherPanel {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();
    let service = cx.weather();
    let muted = |text: String| {
      div()
        .text_xs()
        .text_color(theme.colors.muted_foreground)
        .child(text)
    };

    let Some(weather) = service.current(cx) else {
      let message = service
        .error(cx)
        .map_or("Loading the weather…".to_string(), str::to_string);
      return div()
        .flex()
        .flex_col()
        .size_full()
        .child(card(theme).child(div().flex().justify_center().p_2().child(muted(message))));
    };

    div()
      .flex()
      .flex_col()
      .size_full()
      .gap_2()
      .when_some(service.error(cx), |d, error| {
        d.child(
          card(theme).child(
            div()
              .text_xs()
              .text_color(theme.colors.danger)
              .truncate()
              .child(error.to_string()),
          ),
        )
      })
      .child(
        div()
          .flex()
          .flex_1()
          .min_h_0()
          .gap_2()
          .child(
            div()
              .flex()
              .flex_col()
              .flex_1()
              .min_w_0()
              .gap_2()
              .child(self.hero(theme, weather))
              .child(
                div()
                  .flex()
                  .flex_col()
                  .flex_1()
                  .min_h_0()
                  .overflow_y_scrollbar()
                  .child(self.details(theme, weather)),
              ),
          )
          .child(
            div()
              .flex()
              .flex_1()
              .min_w_0()
              .child(self.forecast(theme, weather, cx)),
          ),
      )
  }
}
