use corona_power::{BatteryState, PowerExt};
use corona_surface::bar::{Button, Widget};
use gpui_kit::{
  Context, Empty, IntoElement, ParentElement, Render, Styled, Subscription, Window, div,
};
use uuid::Uuid;

use crate::control_center::{PowerPanel, Standalone, battery_icon};

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
    let low = low(battery.percentage, battery.state);

    Button::<_, Standalone<PowerPanel>>::new(cx, "battery-button", battery_icon(&battery))
      .danger(low)
      .suffix(
        div()
          .text_xs()
          .pr_1()
          .child(format!("{:.0}%", battery.percentage)),
      )
      .into_any_element()
  }
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
