use std::borrow::Cow;
use std::time::Duration;

use corona_components::async_listener::AsyncListenerExt;
use corona_power::{Battery, BatteryState, PowerExt};
use gpui_kit::{
  Context, IntoElement, ParentElement, Styled,
  assets::IconName,
  base::StyledExt,
  component::{Icon, Theme, progress::Progress, switch::Switch},
  div,
  prelude::FluentBuilder,
};

use crate::{
  control_center::power::{PowerPanel, card},
  i18n::decimal,
};
use rust_i18n::t;

fn duration(duration: Duration) -> String {
  let minutes = (duration.as_secs() + 30) / 60;
  match minutes / 60 {
    0 => t!("app.power.duration.minutes", minutes = minutes).into(),
    hours => t!(
      "app.power.duration.hours",
      hours = hours,
      minutes = minutes % 60
    )
    .into(),
  }
}

pub(crate) fn icon(battery: &Battery) -> IconName {
  match battery.percentage {
    _ if battery.state == BatteryState::Charging => IconName::BatteryCharging,
    p if p <= 10. => IconName::BatteryWarning,
    p if p <= 30. => IconName::BatteryLow,
    p if p <= 70. => IconName::BatteryMedium,
    _ => IconName::BatteryFull,
  }
}

fn state(state: BatteryState) -> Cow<'static, str> {
  match state {
    BatteryState::Charging => t!("app.power.state.charging"),
    BatteryState::Discharging => t!("app.power.state.discharging"),
    BatteryState::Empty => t!("app.power.state.empty"),
    BatteryState::FullyCharged => t!("app.power.state.fully_charged"),
    BatteryState::PendingCharge => t!("app.power.state.pending_charge"),
    BatteryState::PendingDischarge => t!("app.power.state.pending_discharge"),
    BatteryState::Unknown => t!("app.power.state.unknown"),
  }
}

fn detail(theme: &Theme, label: impl IntoElement, value: impl IntoElement) -> impl IntoElement {
  div()
    .flex()
    .w_full()
    .gap_2()
    .items_center()
    .child(div().flex_1().min_w_0().text_sm().truncate().child(label))
    .child(
      div()
        .text_xs()
        .text_color(theme.colors.muted_foreground)
        .child(value),
    )
}

impl PowerPanel {
  pub fn battery(&self, theme: &Theme, cx: &Context<'_, Self>) -> Option<impl IntoElement> {
    let battery = cx.power().battery(cx)?;
    let remaining = battery
      .time_to_empty
      .map(|d| t!("app.power.remaining", duration = duration(d)).to_string())
      .or_else(|| {
        battery
          .time_to_full
          .map(|d| t!("app.power.until_full", duration = duration(d)).to_string())
      });
    let summary = match remaining {
      Some(remaining) => format!("{remaining} · {}", state(battery.state)),
      None => state(battery.state).to_string(),
    };

    Some(
      card(cx)
        .child(
          div()
            .flex()
            .gap_2()
            .items_center()
            .child(Icon::new(icon(battery)))
            .child(
              div()
                .text_sm()
                .font_bold()
                .child(format!("{:.0}%", battery.percentage)),
            )
            .child(
              div()
                .flex_1()
                .min_w_0()
                .text_xs()
                .text_color(theme.colors.muted_foreground)
                .truncate()
                .child(summary),
            ),
        )
        .child(Progress::new("battery-level").value(battery.percentage as f32)),
    )
  }

  pub fn battery_details(&self, theme: &Theme, cx: &Context<'_, Self>) -> Option<impl IntoElement> {
    let battery = cx.power().battery(cx)?;

    Some(
      card(cx)
        .child(div().text_sm().font_bold().child(t!("app.power.battery")))
        .child(detail(
          theme,
          t!("app.power.health"),
          format!("{:.0}%", battery.capacity),
        ))
        .child(detail(
          theme,
          t!("app.power.current_draw"),
          format!("{} W", decimal(battery.energy_rate, 1)),
        ))
        .child(detail(
          theme,
          t!("app.power.capacity"),
          format!(
            "{} / {} Wh",
            decimal(battery.energy_full, 1),
            decimal(battery.energy_full_design, 1)
          ),
        ))
        .when_some(battery.charge_threshold.as_ref(), |d, limit| {
          d.child(detail(
            theme,
            t!("app.power.charge_limit", percent = limit.end),
            Switch::new("charge-limit")
              .checked(limit.enabled)
              .on_change(cx.async_listener(
                |_, enabled: &bool, _, cx| cx.power().set_charge_threshold(*enabled, cx),
                Self::show_error,
              )),
          ))
        }),
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn duration() {
    assert_eq!(
      super::duration(Duration::from_secs(4 * 3600 + 20 * 60)),
      "4 h 20 min"
    );
    assert_eq!(super::duration(Duration::from_secs(25 * 60)), "25 min");
    // rounded to the nearest minute
    assert_eq!(super::duration(Duration::from_secs(3599)), "1 h 0 min");
  }
}
