use corona_power::Battery;
use corona_power::{BatteryState, PowerExt};
use corona_surface::bar::{Button, Widget};
use gpui_kit::{
  Context, Empty, FontWeight, Hsla, IntoElement, ParentElement, Render, Styled, Subscription,
  Window,
  assets::IconName,
  component::{ActiveTheme, Icon},
  div,
  prelude::FluentBuilder,
  px, relative,
};
use uuid::Uuid;

use crate::control_center::{PowerPanel, Standalone};

pub struct BatteryButton {
  _subscription: Subscription,
}

impl Widget for BatteryButton {
  const NAME: &'static str = "battery";
  type Options = ();

  fn init(cx: &mut Context<'_, Self>, _display_id: Uuid, _options: Self::Options) -> Self {
    let battery = cx.power().battery.clone();
    Self {
      _subscription: cx.observe(&battery, |_, _, cx| cx.notify()),
    }
  }
}

/// Flagged red at the level where the icon warns too, unless it charges
fn low(percentage: f64, state: BatteryState) -> bool {
  percentage <= 10. && state != BatteryState::Charging
}

impl Render for BatteryButton {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let Some(battery) = cx.power().battery(cx).cloned() else {
      return Empty.into_any_element();
    };
    let theme = cx.theme();
    let fill = match battery.state {
      BatteryState::Charging => theme.success,
      _ if low(battery.percentage, battery.state) => theme.danger,
      _ => theme.foreground.opacity(0.35),
    };
    let icon = cell(&battery, fill, theme.foreground);

    Button::<_, Standalone<PowerPanel>>::new(cx, "battery-button", IconName::BatteryFull)
      .custom(icon)
      .into_any_element()
  }
}

/// A phone's battery: the level as a fill, its number on top, a bolt while charging
fn cell(battery: &Battery, fill: Hsla, text: Hsla) -> impl IntoElement {
  let level = (battery.percentage / 100.).clamp(0., 1.) as f32;
  let body = div()
    .relative()
    .w(px(25.))
    .h(px(13.))
    .p(px(1.5))
    .rounded(px(4.))
    .border_1()
    .border_color(text.opacity(0.5))
    .child(div().h_full().w(relative(level)).rounded(px(2.)).bg(fill))
    .child(
      div()
        .absolute()
        .inset_0()
        .top(px(1.))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(9.))
        .line_height(px(9.))
        .font_weight(FontWeight::BOLD)
        .text_color(text)
        .child(format!("{:.0}", battery.percentage))
        .when(battery.state == BatteryState::Charging, |d| {
          d.child(Icon::new(IconName::Zap).size(px(7.)).text_color(text))
        }),
    );
  let nub = div()
    .w(px(1.5))
    .h(px(5.))
    .rounded_r(px(1.))
    .bg(text.opacity(0.5));
  div().flex().items_center().child(body).child(nub)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn low() {
    assert!(super::low(10., BatteryState::Discharging));
    assert!(super::low(0., BatteryState::Unknown));
    assert!(!super::low(10.1, BatteryState::Discharging));
    assert!(!super::low(5., BatteryState::Charging));
  }
}
