use corona_bluez::BluetoothExt;
use corona_components::async_listener::AsyncListenerExt;
use corona_components::components::card::{CardExt, ErrorCard};
use gpui_kit::{
  Context, IntoElement, ParentElement, Styled,
  assets::IconName,
  base::{Disableable, StyledExt},
  component::{Icon, Sizable, Theme, button::Button, switch::Switch},
  div,
  prelude::FluentBuilder,
};

use crate::control_center::bluetooth::BluetoothPanel;

impl BluetoothPanel {
  pub fn status(&self, theme: &Theme, cx: &Context<'_, Self>) -> impl IntoElement {
    let bluetooth = cx.bluetooth();
    let adapter = bluetooth.adapter(cx);
    let powered = adapter.is_some_and(|a| a.powered);
    let scanning = adapter.is_some_and(|a| a.discovering);
    let connected = bluetooth
      .list_devices(cx)
      .iter()
      .filter(|d| d.connected)
      .count();
    let subtitle = match (adapter, connected) {
      (None, _) => "No Bluetooth adapter".to_string(),
      (Some(a), _) if !a.powered => "Off".to_string(),
      (_, 0) => "No devices connected".to_string(),
      (_, 1) => "1 device connected".to_string(),
      (_, n) => format!("{n} devices connected"),
    };

    div()
      .flex()
      .flex_col()
      .w_full()
      .gap_2()
      .p_2()
      .card(theme)
      .child(
        div()
          .flex()
          .gap_2()
          .items_center()
          .child(Icon::new(if powered {
            IconName::Bluetooth
          } else {
            IconName::BluetoothOff
          }))
          .child(
            div()
              .flex()
              .flex_col()
              .flex_1()
              .min_w_0()
              .child(div().text_sm().font_bold().child("Bluetooth"))
              .child(
                div()
                  .text_xs()
                  .text_color(theme.colors.muted_foreground)
                  .truncate()
                  .child(subtitle),
              ),
          )
          .child(
            Button::new("bt-scan")
              .icon(IconName::RefreshCw)
              .small()
              .cursor_pointer()
              .tooltip("Scan for devices")
              .disabled(!powered)
              .loading(scanning)
              .on_click(cx.async_listener(|_, _, _, cx| Self::start_scan(cx), Self::show_error)),
          )
          .child(
            Switch::new("bt-powered")
              .checked(powered)
              .disabled(adapter.is_none())
              .on_change(cx.async_listener(
                |_, checked: &bool, _, cx| cx.bluetooth().set_powered(*checked, cx),
                Self::show_error,
              )),
          ),
      )
      .when_some(adapter.filter(|a| a.powered), |d, adapter| {
        d.child(
          div()
            .flex()
            .items_center()
            .child(
              div()
                .flex_1()
                .text_xs()
                .text_color(theme.colors.muted_foreground)
                .child("Discoverable by nearby devices"),
            )
            .child(
              Switch::new("bt-discoverable")
                .checked(adapter.discoverable)
                .on_change(cx.async_listener(
                  |_, checked: &bool, _, cx| cx.bluetooth().set_discoverable(*checked, cx),
                  Self::show_error,
                )),
            ),
        )
      })
  }

  pub fn error(&self, cx: &Context<'_, Self>) -> Option<ErrorCard> {
    let error = self.error.clone()?;
    Some(
      ErrorCard::new("bt-error-dismiss", error).on_dismiss(cx.listener(|this, _, _, cx| {
        this.error = None;
        cx.notify();
      })),
    )
  }
}
