use corona_network_manager::{Interface, NetworkManagerExt, WifiNetwork, WifiStatus};
use corona_components::async_listener::AsyncListenerExt;
use corona_components::components::card::CardExt;
use corona_components::components::list::empty_note;
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  Context, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
  assets::IconName,
  base::{Disableable, StyledExt},
  component::{
    Icon, Sizable, Theme,
    button::{Button, ButtonVariant, ButtonVariants},
    scroll::ScrollableElement,
    spinner::Spinner,
    switch::Switch,
    tag::{Tag, TagVariant},
  },
  div,
  prelude::FluentBuilder,
  px,
};

use crate::panel::network::icons::signal_icon;
use crate::panel::network::{LoadingState, NetworkPanel, refresh_button};
use rust_i18n::t;

impl NetworkPanel {
  pub fn wifi(&self, theme: &Theme, cx: &Context<'_, Self>) -> impl IntoElement {
    let device = cx.network_manager().primary_wifi(cx);
    let enabled = cx.network_manager().wifi_enabled(cx);
    let supported = cx.network_manager().wifi_supported(cx);
    let networks = cx.network_manager().list_wifi_networks(cx);

    let placeholder_text = if !supported {
      Some(t!("app.network.wifi.unsupported"))
    } else if device.is_none() {
      Some(t!("app.network.wifi.disabled"))
    } else if networks.is_empty() {
      Some(t!("app.network.wifi.none_found"))
    } else {
      None
    };

    div()
      .flex()
      .flex_col()
      .w_full()
      .flex_1()
      .min_h(px(120.))
      .gap_2()
      .p_2()
      .card(cx)
      .child(
        div()
          .flex()
          .gap_2()
          .items_center()
          .child(
            div()
              .font_bold()
              .text_sm()
              .child(t!("app.network.wifi.title")),
          )
          .child(
            Button::new("join-hidden-network")
              .icon(IconName::Plus)
              .primary()
              .disabled(device.is_none())
              .small()
              .ml_auto()
              .cursor_pointer()
              .when(device.is_none(), |this| this.cursor_not_allowed())
              .tooltip(t!("app.network.hidden.title"))
              .on_click(cx.listener(|this, _, window, cx| this.open_hidden_prompt(window, cx))),
          )
          .child(
            refresh_button("wifi-rescan", &self.wifi_scanning)
              .disabled(device.is_none())
              .tooltip(t!("app.network.wifi.scan"))
              .on_click(cx.async_listener(
                |this, _, _, cx| {
                  this.wifi_scanning = LoadingState::Loading;
                  cx.network_manager().rescan(cx)
                },
                |this, result, _| this.wifi_scanning = result.log_err().into(),
              )),
          )
          .child(
            Switch::new("wifi-enabled")
              .cursor_pointer()
              .when(!supported, |this| this.cursor_not_allowed())
              .checked(enabled)
              .disabled(!supported)
              .on_change(cx.async_listener(
                |_, checked, _, cx| cx.network_manager().set_wifi_enabled(*checked),
                |this, result, _| this.error.set(result),
              )),
          ),
      )
      .when_some(placeholder_text.clone(), |d, text| {
        d.child(empty_note(text, cx))
      })
      .when_none(&placeholder_text, |d| {
        d.child(
          div()
            .flex()
            .flex_col()
            .gap_1()
            .flex_1()
            .min_h_0()
            .overflow_y_scrollbar()
            .children(networks.iter().map(|n| self.network(theme, n, device, cx))),
        )
      })
  }

  fn network(
    &self,
    theme: &Theme,
    network: &WifiNetwork,
    device: Option<&Interface>,
    cx: &Context<'_, Self>,
  ) -> impl IntoElement {
    let interface = device.map(|d| d.name.clone()).unwrap_or_default();
    let signal_icon = signal_icon(network.strength);

    div()
      .id(format!("wifi-network-{}", network.ssid))
      .flex()
      .gap_2()
      .p_2()
      .w_full()
      .items_center()
      .rounded_xl()
      .bg(theme.colors.background)
      .child(Icon::new(signal_icon).small())
      .child(
        div()
          .flex_1()
          .min_w_0()
          .text_sm()
          .truncate()
          .child(network.ssid.clone()),
      )
      .when(network.secured, |d| {
        d.child(Icon::new(IconName::Lock).small().mr_1())
      })
      .when(network.status == WifiStatus::Connected, |d| {
        d.child(
          Tag::new()
            .small()
            .with_variant(TagVariant::Success)
            .child(t!("app.network.connected")),
        )
      })
      .when(network.status == WifiStatus::Connecting, |d| {
        d.child(Spinner::new().small())
      })
      .when(network.status == WifiStatus::NeedAuth, |d| {
        d.child(
          Tag::new()
            .small()
            .with_variant(TagVariant::Warning)
            .child(t!("app.network.password")),
        )
      })
      .when(network.status == WifiStatus::Saved, |d| {
        d.child(
          Tag::new()
            .small()
            .with_variant(TagVariant::Secondary)
            .child(t!("app.network.wifi.saved")),
        )
      })
      .when_else(
        network.status != WifiStatus::New && network.status != WifiStatus::Saved,
        |d| {
          d.child(
            Button::new(format!("wifi-disconnect-{}", network.ssid))
              .icon(IconName::Unplug)
              .small()
              .with_variant(ButtonVariant::Danger)
              .tooltip(t!("app.common.disconnect"))
              .cursor_pointer()
              .on_click(cx.async_listener(
                move |_, _, _, cx| cx.network_manager().disconnect(&interface, cx),
                |this, result, _| this.error.set(result),
              )),
          )
        },
        |d| {
          d.cursor_pointer().on_click(cx.async_listener(
            {
              let ssid = network.ssid.clone();
              move |_, _, _, cx| cx.network_manager().connect_wifi(ssid.clone(), cx)
            },
            |this, result, _| this.error.set(result),
          ))
        },
      )
      .when(network.status != WifiStatus::New, |d| {
        d.child(
          Button::new(format!("wifi-forget-{}", network.ssid))
            .icon(IconName::Trash)
            .small()
            .with_variant(ButtonVariant::Danger)
            .tooltip(t!("app.common.forget"))
            .cursor_pointer()
            .on_click(cx.async_listener(
              {
                let ssid = network.ssid.clone();
                move |_, _, _, cx| {
                  cx.stop_propagation();
                  cx.network_manager().forget_wifi(ssid.clone(), cx)
                }
              },
              |this, result, _| this.error.set(result),
            )),
        )
      })
  }
}
