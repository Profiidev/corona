use std::time::Duration;

use corona_components::async_listener::AsyncListenerExt;
use corona_power::{Battery, BatteryState, PowerExt};
use gpui_kit::{
  Context, IntoElement, ParentElement, SharedString, Styled,
  assets::IconName,
  base::StyledExt,
  component::{Icon, Theme, progress::Progress, switch::Switch},
  div,
  prelude::FluentBuilder,
};

use crate::control_center::power::{PowerPanel, card};

fn duration(duration: Duration) -> String {
  let minutes = (duration.as_secs() + 30) / 60;
  match minutes / 60 {
    0 => format!("{minutes} min"),
    hours => format!("{hours} h {} min", minutes % 60),
  }
}

fn icon(battery: &Battery) -> IconName {
  match battery.percentage {
    _ if battery.state == BatteryState::Charging => IconName::BatteryCharging,
    p if p <= 10. => IconName::BatteryWarning,
    p if p <= 30. => IconName::BatteryLow,
    p if p <= 70. => IconName::BatteryMedium,
    _ => IconName::BatteryFull,
  }
}

fn state(state: BatteryState) -> &'static str {
  match state {
    BatteryState::Charging => "charging",
    BatteryState::Discharging => "discharging",
    BatteryState::Empty => "empty",
    BatteryState::FullyCharged => "fully charged",
    BatteryState::PendingCharge => "pending charge",
    BatteryState::PendingDischarge => "pending discharge",
    BatteryState::Unknown => "unknown",
  }
}

fn detail(
  theme: &Theme,
  label: impl Into<SharedString>,
  value: impl IntoElement,
) -> impl IntoElement {
  div()
    .flex()
    .w_full()
    .gap_2()
    .items_center()
    .child(
      div()
        .flex_1()
        .min_w_0()
        .text_sm()
        .truncate()
        .child(label.into()),
    )
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
      .map(|d| format!("{} remaining", duration(d)))
      .or_else(|| {
        battery
          .time_to_full
          .map(|d| format!("{} until full", duration(d)))
      });
    let summary = match remaining {
      Some(remaining) => format!("{remaining} · {}", state(battery.state)),
      None => state(battery.state).to_string(),
    };

    Some(
      card(theme)
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
      card(theme)
        .child(div().text_sm().font_bold().child("Battery"))
        .child(detail(theme, "Health", format!("{:.0}%", battery.capacity)))
        .child(detail(
          theme,
          "Current draw",
          format!("{:.1} W", battery.energy_rate),
        ))
        .child(detail(
          theme,
          "Capacity",
          format!(
            "{:.1} / {:.1} Wh",
            battery.energy_full, battery.energy_full_design
          ),
        ))
        .when_some(battery.charge_threshold.as_ref(), |d, limit| {
          d.child(detail(
            theme,
            format!("Limit charging to {}%", limit.end),
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
