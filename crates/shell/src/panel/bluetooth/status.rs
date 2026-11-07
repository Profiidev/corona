use corona_bluez::BluetoothExt;
use corona_components::async_listener::AsyncListenerExt;
use corona_components::components::card::CardExt;
use gpui_kit::{
  Context, IntoElement, ParentElement, Styled,
  assets::IconName,
  base::{Disableable, StyledExt},
  component::{Icon, Sizable, Theme, button::Button, switch::Switch},
  div,
  prelude::FluentBuilder,
};

use crate::panel::bluetooth::BluetoothPanel;
use rust_i18n::t;

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
      (None, _) => t!("app.bluetooth.no_adapter").to_string(),
      (Some(a), _) if !a.powered => t!("app.common.off").to_string(),
      (_, 0) => t!("app.bluetooth.connected.none").to_string(),
      (_, 1) => t!("app.bluetooth.connected.one").to_string(),
      (_, n) => t!("app.bluetooth.connected.other", count = n).to_string(),
    };

    div()
      .flex()
      .flex_col()
      .w_full()
      .gap_2()
      .p_2()
      .card(cx)
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
              .child(div().text_sm().font_bold().child(t!("app.bluetooth.title")))
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
              .when(!powered, |this| this.cursor_not_allowed())
              .tooltip(t!("app.bluetooth.scan"))
              .disabled(!powered)
              .loading(scanning)
              .on_click(cx.async_listener(
                |_, _, _, cx| Self::start_scan(cx),
                |this, result, _| this.error.set(result),
              )),
          )
          .child(
            Switch::new("bt-powered")
              .cursor_pointer()
              .when(adapter.is_none(), |this| this.cursor_not_allowed())
              .checked(powered)
              .disabled(adapter.is_none())
              .on_change(cx.async_listener(
                |_, checked: &bool, _, cx| cx.bluetooth().set_powered(*checked, cx),
                |this, result, _| this.error.set(result),
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
                .child(t!("app.bluetooth.discoverable")),
            )
            .child(
              Switch::new("bt-discoverable")
                .cursor_pointer()
                .checked(adapter.discoverable)
                .on_change(cx.async_listener(
                  |_, checked: &bool, _, cx| cx.bluetooth().set_discoverable(*checked, cx),
                  |this, result, _| this.error.set(result),
                )),
            ),
        )
      })
  }
}
