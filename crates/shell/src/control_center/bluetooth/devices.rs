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

const TYPES: [(&str, IconName, &str); 11] = [
  ("audio-headset", IconName::Headphones, "Headset"),
  ("audio-headphones", IconName::Headphones, "Headphones"),
  ("audio-card", IconName::Speaker, "Speaker"),
  ("input-gaming", IconName::Gamepad2, "Gamepad"),
  ("input-keyboard", IconName::Keyboard, "Keyboard"),
  ("input-mouse", IconName::Mouse, "Mouse"),
  ("input-tablet", IconName::Tablet, "Tablet"),
  ("phone", IconName::Smartphone, "Phone"),
  ("computer", IconName::Laptop, "Computer"),
  ("video-display", IconName::Monitor, "Display"),
  ("printer", IconName::Printer, "Printer"),
];

fn device_type(device: &Device) -> (IconName, &'static str) {
  let icon = device.icon.as_deref().unwrap_or_default();
  TYPES
    .iter()
    .find(|(prefix, ..)| icon.starts_with(prefix))
    .map_or((IconName::Bluetooth, "Device"), |(_, icon, label)| {
      (*icon, *label)
    })
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
      .then(|| placeholder(theme, "No paired devices"));
    self.devices(theme, "Paired devices", devices, empty, cx)
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
            .child("Scanning…"),
        )
    });
    Some(self.devices(theme, "Available devices", devices, empty, cx))
  }

  fn devices(
    &self,
    theme: &Theme,
    title: &'static str,
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
      .card(theme)
      .child(div().font_bold().text_sm().child(title))
      .when_some(empty, |d, empty| d.child(empty))
      .child(
        div()
          .flex()
          .flex_col()
          .gap_1()
          .h_auto()
          .overflow_y_scrollbar()
          .id(title)
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
        .tooltip("Pair")
        .with_variant(ButtonVariant::Primary)
    } else if device.connected {
      Button::new(format!("bt-disconnect-{address}"))
        .icon(IconName::Unplug)
        .tooltip("Disconnect")
        .with_variant(ButtonVariant::Danger)
    } else {
      Button::new(format!("bt-connect-{address}"))
        .icon(IconName::Plug)
        .tooltip("Connect")
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
            .tooltip("Forget")
            .cursor_pointer()
            .on_click(cx.async_listener(
              move |_, _, _, cx| cx.bluetooth().forget(&address, cx),
              Self::show_error,
            )),
        )
      })
  }
}

fn placeholder(theme: &Theme, text: &'static str) -> impl IntoElement {
  div()
    .flex()
    .justify_center()
    .p_2()
    .text_xs()
    .text_color(theme.colors.muted_foreground)
    .child(text)
}
