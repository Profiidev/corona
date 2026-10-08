use corona_components::components::card::CardExt;
use corona_weather::{Condition, Location, WeatherExt};
use gpui_kit::{
  AnyElement, App, Context, Div, IntoElement, ParentElement, Render, Styled, Subscription, Window,
  assets::IconName,
  component::{ActiveTheme, button::Button, scroll::ScrollableElement},
  div,
  prelude::FluentBuilder,
};

use crate::control_center::{ControlCenterPanel, variants::ControlCenterType};
use rust_i18n::t;

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
        .tooltip(t!("app.weather.refresh"))
        .cursor_pointer()
        .on_click(|_, _, cx| cx.weather().refresh())
        .into_any_element(),
    ]
  }
}

fn card(cx: &App) -> Div {
  div().flex().flex_col().w_full().gap_2().p_2().card(cx)
}

/// The condition in words, like "Partly cloudy"
pub(crate) fn describe(condition: Condition) -> String {
  t!(format!("app.weather.condition.{}", condition.key())).into()
}

/// The location's name; the device's own position has none
pub(crate) fn place_name(location: &Location) -> String {
  match location.name.is_empty() {
    true => t!("app.weather.current_location").into(),
    false => location.name.clone(),
  }
}

pub(super) fn icon(condition: Condition, is_day: bool) -> IconName {
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
        .map_or(t!("app.weather.loading").to_string(), str::to_string);
      return div()
        .flex()
        .flex_col()
        .size_full()
        .child(card(cx).child(div().flex().justify_center().p_2().child(muted(message))));
    };

    div()
      .flex()
      .flex_col()
      .size_full()
      .gap_2()
      .when_some(service.error(cx), |d, error| {
        d.child(
          card(cx).child(
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
              .child(self.hero(cx, weather))
              .child(
                div()
                  .flex()
                  .flex_col()
                  .flex_1()
                  .min_h_0()
                  .overflow_y_scrollbar()
                  .child(self.details(cx, weather)),
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

#[cfg(test)]
mod tests {
  use super::*;

  fn location(name: &str) -> Location {
    Location {
      name: name.into(),
      latitude: 0.,
      longitude: 0.,
      query: None,
    }
  }

  #[test]
  fn icon_day_night() {
    for condition in Condition::ALL {
      let differs = matches!(
        condition,
        Condition::Clear | Condition::MainlyClear | Condition::PartlyCloudy
      );
      assert_eq!(
        icon(condition, true) != icon(condition, false),
        differs,
        "{condition:?}"
      );
    }
    assert_eq!(icon(Condition::Clear, true), IconName::Sun);
    assert_eq!(icon(Condition::Clear, false), IconName::Moon);
    assert_eq!(icon(Condition::PartlyCloudy, false), IconName::CloudMoon);
    assert_eq!(icon(Condition::Unknown, true), IconName::Cloud);
  }

  #[test]
  fn describe_every_condition() {
    for condition in Condition::ALL {
      let text = describe(condition);
      assert!(!text.is_empty() && !text.contains('.'), "{text}");
    }
  }

  #[test]
  fn place_name() {
    assert_eq!(super::place_name(&location("Berlin")), "Berlin");
    assert_eq!(super::place_name(&location("")), "Current location");
  }
}
