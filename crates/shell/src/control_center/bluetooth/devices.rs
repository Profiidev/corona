use std::borrow::Cow;
use std::pin::Pin;

use corona_bluez::{BluetoothExt, Device};
use corona_components::async_listener::AsyncListenerExt;
use corona_components::components::card::CardExt;
use gpui_kit::{
  Context, IntoElement, ParentElement, Styled,
  assets::IconName,
  base::StyledExt,
  component::{
    Icon, Sizable, Theme,
    button::{Button, ButtonVariant, ButtonVariants},
    scroll::ScrollableElement,
    spinner::Spinner,
  },
  div,
  prelude::FluentBuilder,
  px,
};

use crate::control_center::bluetooth::BluetoothPanel;
use rust_i18n::t;

const TYPES: [(&str, IconName, &str); 11] = [
  (
    "audio-headset",
    IconName::Headphones,
    "app.bluetooth.type.headset",
  ),
  (
    "audio-headphones",
    IconName::Headphones,
    "app.bluetooth.type.headphones",
  ),
  (
    "audio-card",
    IconName::Speaker,
    "app.bluetooth.type.speaker",
  ),
  (
    "input-gaming",
    IconName::Gamepad2,
    "app.bluetooth.type.gamepad",
  ),
  (
    "input-keyboard",
    IconName::Keyboard,
    "app.bluetooth.type.keyboard",
  ),
  ("input-mouse", IconName::Mouse, "app.bluetooth.type.mouse"),
  (
    "input-tablet",
    IconName::Tablet,
    "app.bluetooth.type.tablet",
  ),
  ("phone", IconName::Smartphone, "app.bluetooth.type.phone"),
  ("computer", IconName::Laptop, "app.bluetooth.type.computer"),
  (
    "video-display",
    IconName::Monitor,
    "app.bluetooth.type.display",
  ),
  ("printer", IconName::Printer, "app.bluetooth.type.printer"),
];

fn device_type(device: &Device) -> (IconName, Cow<'static, str>) {
  let icon = device.icon.as_deref().unwrap_or_default();
  TYPES
    .iter()
    .find(|(prefix, ..)| icon.starts_with(prefix))
    .map_or(
      (IconName::Bluetooth, t!("app.bluetooth.type.device")),
      |(_, icon, key)| (*icon, t!(*key)),
    )
}

impl BluetoothPanel {
  pub fn paired(&self, theme: &Theme, cx: &Context<'_, Self>) -> impl IntoElement {
    let devices: Vec<&Device> = cx
      .bluetooth()
      .list_devices(cx)
      .iter()
      .filter(|d| d.paired)
      .collect();
    let empty = devices
      .is_empty()
      .then(|| placeholder(theme, t!("app.bluetooth.no_paired")));
    self.devices(
      theme,
      "bt-paired",
      t!("app.bluetooth.paired"),
      devices,
      empty,
      cx,
    )
  }

  pub fn available(&self, theme: &Theme, cx: &Context<'_, Self>) -> Option<impl IntoElement> {
    let bluetooth = cx.bluetooth();
    let scanning = bluetooth.adapter(cx).is_some_and(|a| a.discovering);
    let devices: Vec<&Device> = bluetooth
      .list_devices(cx)
      .iter()
      .filter(|d| !d.paired)
      .collect();
    if devices.is_empty() && !scanning {
      return None;
    }
    let empty = devices.is_empty().then(|| {
      div()
        .flex()
        .gap_2()
        .justify_center()
        .items_center()
        .p_2()
        .child(Spinner::new().small())
        .child(
          div()
            .text_xs()
            .text_color(theme.colors.muted_foreground)
            .child(t!("app.bluetooth.scanning")),
        )
    });
    Some(self.devices(
      theme,
      "bt-available",
      t!("app.bluetooth.available"),
      devices,
      empty,
      cx,
    ))
  }

  fn devices(
    &self,
    theme: &Theme,
    id: &'static str,
    title: impl IntoElement,
    devices: Vec<&Device>,
    empty: Option<impl IntoElement>,
    cx: &Context<'_, Self>,
  ) -> impl IntoElement {
    div()
      .flex()
      .flex_col()
      .w_full()
      .when_else(
        devices.len() > 2,
        |d| d.min_h(px(128.)),
        |d| d.flex_shrink_0(),
      )
      .gap_2()
      .p_2()
      .card(cx)
      .child(div().font_bold().text_sm().child(title))
      .when_some(empty, |d, empty| d.child(empty))
      .child(
        div()
          .flex()
          .flex_col()
          .gap_1()
          .h_auto()
          .overflow_y_scrollbar()
          .id(id)
          .children(devices.into_iter().map(|d| self.device(theme, d, cx))),
      )
  }

  fn device(&self, theme: &Theme, device: &Device, cx: &Context<'_, Self>) -> impl IntoElement {
    let (icon, label) = device_type(device);
    let details = match device.battery {
      Some(battery) => format!("{label} · {battery}%"),
      None => label.to_string(),
    };
    let address = device.address.clone();

    let action = if !device.paired {
      Button::new(format!("bt-pair-{address}"))
        .icon(IconName::Link)
        .tooltip(t!("app.bluetooth.pair"))
        .with_variant(ButtonVariant::Primary)
    } else if device.connected {
      Button::new(format!("bt-disconnect-{address}"))
        .icon(IconName::Unplug)
        .tooltip(t!("app.common.disconnect"))
        .with_variant(ButtonVariant::Danger)
    } else {
      Button::new(format!("bt-connect-{address}"))
        .icon(IconName::Plug)
        .tooltip(t!("app.common.connect"))
        .with_variant(ButtonVariant::Primary)
    };
    let (paired, connected) = (device.paired, device.connected);

    div()
      .flex()
      .w_full()
      .gap_2()
      .p_2()
      .items_center()
      .rounded_xl()
      .bg(theme.colors.background)
      .child(
        Icon::new(icon)
          .small()
          .when(connected, |i| i.text_color(theme.colors.primary)),
      )
      .child(
        div()
          .flex()
          .flex_col()
          .flex_1()
          .min_w_0()
          .child(div().text_sm().truncate().child(device.name.clone()))
          .child(
            div()
              .text_xs()
              .text_color(theme.colors.muted_foreground)
              .truncate()
              .child(details),
          ),
      )
      .child(action.small().cursor_pointer().on_click(cx.async_listener(
        {
          let address = address.clone();
          move |_, _, _, cx| {
            let bluetooth = cx.bluetooth();
            let task: Pin<Box<dyn Future<Output = anyhow::Result<()>>>> = if !paired {
              Box::pin(bluetooth.pair(&address, cx))
            } else if connected {
              Box::pin(bluetooth.disconnect(&address, cx))
            } else {
              Box::pin(bluetooth.connect(&address, cx))
            };
            task
          }
        },
        Self::show_error,
      )))
      .when(paired, |d| {
        d.child(
          Button::new(format!("bt-forget-{address}"))
            .icon(IconName::Trash)
            .small()
            .with_variant(ButtonVariant::Danger)
            .tooltip(t!("app.common.forget"))
            .cursor_pointer()
            .on_click(cx.async_listener(
              move |_, _, _, cx| cx.bluetooth().forget(&address, cx),
              Self::show_error,
            )),
        )
      })
  }
}

fn placeholder(theme: &Theme, text: impl IntoElement) -> impl IntoElement {
  div()
    .flex()
    .justify_center()
    .p_2()
    .text_xs()
    .text_color(theme.colors.muted_foreground)
    .child(text)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn device(icon: Option<&str>) -> Device {
    Device {
      path: zbus::zvariant::OwnedObjectPath::try_from("/test").unwrap(),
      address: "00:11:22:33:44:55".into(),
      name: "Test".into(),
      icon: icon.map(Into::into),
      paired: false,
      connected: false,
      trusted: false,
      battery: None,
      rssi: None,
    }
  }

  #[test]
  fn device_type() {
    let kind = |icon| super::device_type(&device(icon));
    assert_eq!(kind(Some("audio-headset")).0, IconName::Headphones);
    assert_eq!(kind(Some("input-gaming")).0, IconName::Gamepad2);
    // bluez icons are matched by prefix
    assert_eq!(kind(Some("phone-apple")).0, IconName::Smartphone);
    assert_eq!(kind(Some("audio-headset")).1, "Headset");
    assert_eq!(
      kind(Some("unknown")),
      (IconName::Bluetooth, "Device".into())
    );
    assert_eq!(kind(None), kind(Some("unknown")));
    assert_eq!(kind(Some("")), kind(None));
  }

  #[test]
  fn device_type_labels_distinct() {
    let mut labels: Vec<_> = TYPES.iter().map(|(_, _, key)| t!(*key)).collect();
    labels.sort();
    labels.dedup();
    assert_eq!(labels.len(), TYPES.len());
  }
}
