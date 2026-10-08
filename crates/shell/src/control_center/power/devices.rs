use corona_power::{BatteryLevel, BatteryType, PowerDevice, PowerExt};
use gpui_kit::{
  Context, IntoElement, ParentElement, Styled,
  assets::IconName,
  base::StyledExt,
  component::{Icon, Sizable, Theme},
  div,
};

use crate::control_center::power::{PowerPanel, card};
use rust_i18n::t;

fn icon(kind: BatteryType) -> IconName {
  match kind {
    BatteryType::Mouse => IconName::Mouse,
    BatteryType::Keyboard => IconName::Keyboard,
    BatteryType::Headset | BatteryType::Headphones => IconName::Headphones,
    BatteryType::Speakers => IconName::Speaker,
    BatteryType::GamingInput => IconName::Gamepad2,
    BatteryType::Phone => IconName::Smartphone,
    BatteryType::Tablet => IconName::Tablet,
    BatteryType::Pen => IconName::Pen,
    BatteryType::Wearable => IconName::Watch,
    _ => IconName::Battery,
  }
}

fn charge(device: &PowerDevice) -> String {
  if device.percentage > 0. {
    return format!("{:.0}%", device.percentage);
  }
  match device.level {
    BatteryLevel::Full => t!("app.power.level.full"),
    BatteryLevel::High => t!("app.power.level.high"),
    BatteryLevel::Normal => t!("app.power.level.normal"),
    BatteryLevel::Low => t!("app.power.level.low"),
    BatteryLevel::Critical => t!("app.power.level.critical"),
    BatteryLevel::None | BatteryLevel::Unknown => t!("app.common.unknown"),
  }
  .into()
}

impl PowerPanel {
  pub fn devices(&self, theme: &Theme, cx: &Context<'_, Self>) -> Option<impl IntoElement> {
    let devices = cx.power().list_devices(cx);
    if devices.is_empty() {
      return None;
    }

    Some(
      card(cx)
        .child(
          div()
            .text_sm()
            .font_bold()
            .child(t!("app.power.connected_devices")),
        )
        .children(devices.iter().map(|device| {
          div()
            .flex()
            .w_full()
            .gap_2()
            .p_2()
            .items_center()
            .rounded_xl()
            .bg(theme.colors.background)
            .child(Icon::new(icon(device.kind)).small())
            .child(
              div()
                .flex_1()
                .min_w_0()
                .text_sm()
                .truncate()
                .child(device.model.clone()),
            )
            .child(
              div()
                .text_xs()
                .text_color(theme.colors.muted_foreground)
                .child(charge(device)),
            )
        })),
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn device(percentage: f64, level: BatteryLevel) -> PowerDevice {
    PowerDevice {
      path: zbus::zvariant::OwnedObjectPath::try_from("/test").unwrap(),
      model: "Test".into(),
      kind: BatteryType::Mouse,
      percentage,
      state: corona_power::BatteryState::Discharging,
      level,
    }
  }

  #[test]
  fn icon() {
    assert_eq!(super::icon(BatteryType::Mouse), IconName::Mouse);
    assert_eq!(
      super::icon(BatteryType::Headset),
      super::icon(BatteryType::Headphones)
    );
    assert_eq!(super::icon(BatteryType::Wearable), IconName::Watch);
    assert_eq!(super::icon(BatteryType::Printer), IconName::Battery);
    assert_eq!(super::icon(BatteryType::Unknown), IconName::Battery);
  }

  #[test]
  fn charge_percentage() {
    assert_eq!(charge(&device(42., BatteryLevel::Unknown)), "42%");
    assert_eq!(charge(&device(99.6, BatteryLevel::Unknown)), "100%");
  }

  #[test]
  fn charge_falls_back_to_level() {
    assert_eq!(charge(&device(0., BatteryLevel::High)), "High");
    assert_eq!(
      charge(&device(0., BatteryLevel::None)),
      charge(&device(0., BatteryLevel::Unknown))
    );
    let levels = [
      BatteryLevel::Full,
      BatteryLevel::High,
      BatteryLevel::Normal,
      BatteryLevel::Low,
      BatteryLevel::Critical,
      BatteryLevel::Unknown,
    ];
    let mut labels: Vec<_> = levels.iter().map(|l| charge(&device(0., *l))).collect();
    labels.sort();
    labels.dedup();
    assert_eq!(labels.len(), levels.len());
  }
}
