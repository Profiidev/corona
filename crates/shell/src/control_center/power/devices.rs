use corona_power::{BatteryLevel, BatteryType, PowerDevice, PowerExt};
use gpui_kit::{
  Context, IntoElement, ParentElement, Styled,
  assets::IconName,
  base::StyledExt,
  component::{Icon, Sizable, Theme},
  div,
};

use crate::control_center::power::{PowerPanel, card};

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
    BatteryLevel::Full => "Full",
    BatteryLevel::High => "High",
    BatteryLevel::Normal => "Normal",
    BatteryLevel::Low => "Low",
    BatteryLevel::Critical => "Critical",
    BatteryLevel::None | BatteryLevel::Unknown => "Unknown",
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
        .child(div().text_sm().font_bold().child("Connected devices"))
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
